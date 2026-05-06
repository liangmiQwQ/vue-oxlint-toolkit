use oxc_allocator::{Box as ArenaBox, Vec as ArenaVec};
use oxc_span::Span;

use crate::ast::{
  VAttribute, VDirective, VDirectiveArgument, VDirectiveExpression, VDirectiveKey, VForDirective,
  VIdentifier, VLiteral, VOnDirective, VPureAttribute, VSlotDirective,
};
use crate::lexer::{VToken, VTokenKind};
use crate::parser::parse::TemplateParser;
use crate::parser::parse::utils::{is_directive_name, split_directive_argument};

type DirectiveKeyParts<'a, 'b> =
  (&'a VIdentifier<'a>, VDirectiveArgument<'a, 'b>, ArenaVec<'a, VIdentifier<'a>>);

pub(super) struct ParsedAttribute<'a, 'b> {
  pub(super) ast: Option<VAttribute<'a, 'b>>,
  pub(super) name: &'b str,
  pub(super) value: Option<&'b str>,
}

impl<'a, 'b> TemplateParser<'_, 'a, 'b>
where
  'b: 'a,
{
  pub(super) fn parse_attribute(&mut self) -> ParsedAttribute<'a, 'b> {
    let Some(first) = self.next() else {
      return ParsedAttribute { ast: None, name: "", value: None };
    };

    self.parser.sfc.template_tokens.push(first.into());
    let raw_start = first.span.start;
    let mut raw_end = first.span.end;
    let mut raw_name_end = first.span.end;
    let mut value_span = None;
    let name = if first.kind == VTokenKind::Punctuator {
      if let Some(next) = self.next_non_ws() {
        if next.kind == VTokenKind::HTMLIdentifier {
          self.parser.sfc.template_tokens.push(next.into());
          raw_end = next.span.end;
          raw_name_end = next.span.end;
          &self.parser.source_text[raw_start as usize..raw_name_end as usize]
        } else {
          self.peeked = Some(next);
          first.value.unwrap_or_default()
        }
      } else {
        first.value.unwrap_or_default()
      }
    } else {
      first.value.unwrap_or_default()
    };

    while let Some(token) = self.peek() {
      if token.kind != VTokenKind::Punctuator {
        break;
      }
      // SAFETY: `peek()` proved the token exists and is a punctuator.
      let token = self.next().unwrap();
      raw_end = token.span.end;
      raw_name_end = token.span.end;
      self.parser.sfc.template_tokens.push(token.into());

      if let Some(part) = self.next_non_ws() {
        if part.kind == VTokenKind::HTMLIdentifier {
          raw_end = part.span.end;
          raw_name_end = part.span.end;
          self.parser.sfc.template_tokens.push(part.into());
        } else {
          self.peeked = Some(part);
          break;
        }
      }
    }

    let value = if self.peek().is_some_and(|token| token.kind == VTokenKind::HTMLAssociation) {
      // SAFETY: the guard above proves the next token is the association token.
      let eq = self.next().unwrap();
      self.parser.sfc.template_tokens.push(eq.into());
      if let Some(value_token) = self.next_non_ws() {
        raw_end = value_token.span.end;
        value_span = Some(value_token.value_span());
        self.parser.sfc.template_tokens.push(value_token.into());
        value_token.value
      } else {
        None
      }
    } else {
      None
    };

    let raw_name = &self.parser.source_text[raw_start as usize..raw_name_end as usize];
    if is_directive_name(raw_name)
      && !is_plain_value_attribute(raw_name, value_span)
      && (value_span.is_some() || !raw_name.starts_with(':'))
      && let Some(ast) =
        self.parse_directive_attribute(raw_name, Span::new(raw_start, raw_end), value_span)
    {
      return ParsedAttribute { ast: Some(ast), name, value };
    }

    let key_name_source =
      if is_plain_value_attribute(raw_name, value_span) || matches!(raw_name, ":" | "#") {
        raw_name
      } else {
        name.trim_start_matches([':', '@', '#'])
      };
    let key_name = self.alloc_value(key_name_source);
    let raw_name = self.alloc_value(raw_name);
    let value_node = value.map(|value| {
      let value = self.alloc_value(value);
      VLiteral { value, span: Span::new(raw_name_end, raw_end) }
    });
    let attr = VPureAttribute {
      key: VIdentifier { name: key_name, raw_name, span: Span::new(raw_start, raw_name_end) },
      value: value_node,
      span: Span::new(raw_start, raw_end),
    };

    ParsedAttribute {
      ast: Some(VAttribute::VPureAttribute(ArenaBox::new_in(attr, self.parser.vue_allocator))),
      name,
      value,
    }
  }

  fn parse_directive_attribute(
    &mut self,
    raw_name: &'b str,
    attr_span: Span,
    value_span: Option<Span>,
  ) -> Option<VAttribute<'a, 'b>> {
    let (name, argument, modifiers) = self.parse_directive_key(raw_name, attr_span)?;
    let directive_name = name.name;

    if directive_name == "for"
      && let Some(value_span) = value_span
      && let Some(value) = self.parse_v_for_expression(value_span)
    {
      let directive = VForDirective {
        key: VDirectiveKey {
          name,
          argument,
          modifiers,
          span: Span::sized(attr_span.start, raw_name.len() as u32),
        },
        value,
        span: attr_span,
      };

      return Some(VAttribute::VForDirective(ArenaBox::new_in(
        directive,
        self.parser.vue_allocator,
      )));
    }

    if directive_name == "slot"
      && let Some(value_span) = value_span
      && let Some(value) = self.parse_v_slot_expression(value_span)
    {
      let directive = VSlotDirective {
        key: VDirectiveKey {
          name,
          argument,
          modifiers,
          span: Span::sized(attr_span.start, raw_name.len() as u32),
        },
        value,
        span: attr_span,
      };

      return Some(VAttribute::VSlotDirective(ArenaBox::new_in(
        directive,
        self.parser.vue_allocator,
      )));
    }

    if directive_name == "on"
      && let Some(value_span) = value_span
      && let Some(value) = self.parse_v_on_expression(value_span)
    {
      let directive = VOnDirective {
        key: VDirectiveKey {
          name,
          argument,
          modifiers,
          span: Span::sized(attr_span.start, raw_name.len() as u32),
        },
        value,
        span: attr_span,
      };

      return Some(VAttribute::VOnDirective(ArenaBox::new_in(
        directive,
        self.parser.vue_allocator,
      )));
    }

    let value = if let Some(value_span) = value_span {
      let (expression, references, tokens) = self.parser.parse_pure_expression(value_span)?;
      if !tokens.is_empty() {
        self.parser.sfc.template_tokens.push(tokens.into());
      }
      Some(VDirectiveExpression { expression, references, span: value_span })
    } else {
      None
    };

    let directive = VDirective {
      key: VDirectiveKey {
        name,
        argument,
        modifiers,
        span: Span::sized(attr_span.start, raw_name.len() as u32),
      },
      value,
      span: attr_span,
    };

    Some(VAttribute::VDirective(ArenaBox::new_in(directive, self.parser.vue_allocator)))
  }

  fn parse_directive_key(
    &self,
    raw_name: &'b str,
    attr_span: Span,
  ) -> Option<DirectiveKeyParts<'a, 'b>> {
    let parsed = ParsedDirectiveName::new(raw_name, attr_span)?;
    let name = self.parser.vue_allocator.alloc(VIdentifier {
      name: self.alloc_value(parsed.name),
      raw_name: self.alloc_value(parsed.raw_name),
      span: parsed.name_span,
    });

    let mut modifiers = ArenaVec::new_in(self.parser.vue_allocator);
    let (argument_source, modifier_source) = split_directive_argument(parsed.rest);
    let argument = argument_source.map_or_else(
      || {
        VDirectiveArgument::VIdentifier(ArenaBox::new_in(
          VIdentifier {
            name: "",
            raw_name: "",
            span: Span::new(parsed.rest_start, parsed.rest_start),
          },
          self.parser.vue_allocator,
        ))
      },
      |(argument_source, argument_offset)| {
        let arg_start = parsed.rest_start + argument_offset as u32;
        VDirectiveArgument::VIdentifier(ArenaBox::new_in(
          VIdentifier {
            name: self.alloc_value(argument_source),
            raw_name: self.alloc_value(argument_source),
            span: Span::sized(arg_start, argument_source.len() as u32),
          },
          self.parser.vue_allocator,
        ))
      },
    );

    for (modifier, modifier_offset) in modifier_source {
      let modifier_start = parsed.rest_start + modifier_offset as u32;
      modifiers.push(VIdentifier {
        name: self.alloc_value(modifier),
        raw_name: self.alloc_value(modifier),
        span: Span::sized(modifier_start, modifier.len() as u32),
      });
    }

    Some((name, argument, modifiers))
  }
}

struct ParsedDirectiveName<'b> {
  name: &'b str,
  raw_name: &'b str,
  rest: &'b str,
  rest_start: u32,
  name_span: Span,
}

impl<'b> ParsedDirectiveName<'b> {
  fn new(raw_name: &'b str, attr_span: Span) -> Option<Self> {
    if let Some(rest) = raw_name.strip_prefix(':') {
      return Some(Self {
        name: "bind",
        raw_name: ":",
        rest,
        rest_start: attr_span.start + 1,
        name_span: Span::sized(attr_span.start, 1),
      });
    }

    if let Some(rest) = raw_name.strip_prefix('@') {
      return Some(Self {
        name: "on",
        raw_name: "@",
        rest,
        rest_start: attr_span.start + 1,
        name_span: Span::sized(attr_span.start, 1),
      });
    }

    if let Some(rest) = raw_name.strip_prefix('#') {
      return Some(Self {
        name: "slot",
        raw_name: "#",
        rest,
        rest_start: attr_span.start + 1,
        name_span: Span::sized(attr_span.start, 1),
      });
    }

    let rest = raw_name.strip_prefix("v-")?;
    let split = rest.find([':', '.']).unwrap_or(rest.len());
    let name = &rest[..split];
    Some(Self {
      name,
      raw_name: name,
      rest: &rest[split..],
      rest_start: attr_span.start + 2 + split as u32,
      name_span: Span::sized(attr_span.start, split as u32 + 2),
    })
  }
}

fn is_plain_value_attribute(raw_name: &str, value_span: Option<Span>) -> bool {
  value_span.is_some() && matches!(raw_name, ":" | "#" | "v-slot:")
}

trait TokenValueSpan {
  fn value_span(self) -> Span;
}

impl TokenValueSpan for VToken<'_> {
  fn value_span(self) -> Span {
    if self.kind == VTokenKind::HTMLLiteral && self.span.end > self.span.start + 1 {
      Span::new(self.span.start + 1, self.span.end - 1)
    } else {
      self.span
    }
  }
}
