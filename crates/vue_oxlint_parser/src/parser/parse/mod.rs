use oxc_allocator::{Allocator, Box as ArenaBox, CloneIn, Vec as ArenaVec};
use oxc_ast::ast::{BindingPattern, Expression, FormalParameters, Statement};
use oxc_span::{GetSpan, SourceType, Span};

use crate::ast::{
  VAttribute, VComment, VDirective, VDirectiveArgument, VDirectiveExpression, VDirectiveKey,
  VElement, VEndTag, VForDirective, VForExpression, VIdentifier, VLiteral, VNode, VOnDirective,
  VOnExpression, VPureAttribute, VPureScript, VSlotDirective, VSlotExpression, VStartTag, VText,
  Variable,
};
use crate::lexer::{Lexer, VToken, VTokenKind};
use crate::parser::module_record::merge_module_record;
use crate::{VueParser, error};

pub struct TemplateParser<'p, 'a, 'b>
where
  'b: 'a,
{
  parser: &'p mut VueParser<'a, 'b>,
  lexer: Lexer<'b>,
  peeked: Option<VToken<'b>>,
}

struct ParsedAttribute<'a, 'b> {
  ast: Option<VAttribute<'a, 'b>>,
  name: &'b str,
  value: Option<&'b str>,
}

struct ParsedStartTag<'a, 'b> {
  name: &'a str,
  raw_name: &'a str,
  raw_name_source: &'b str,
  attributes: ArenaVec<'a, VAttribute<'a, 'b>>,
  self_closing: bool,
  has_v_pre: bool,
  span: Span,
}

impl<'p, 'a, 'b> TemplateParser<'p, 'a, 'b>
where
  'b: 'a,
{
  pub const fn new(parser: &'p mut VueParser<'a, 'b>) -> Self {
    Self { lexer: Lexer::new(parser.source_text), parser, peeked: None }
  }

  pub fn parse(&mut self) -> bool {
    let children = self.parse_children(None);
    self.parser.sfc.children = children;
    self.lexer.panicked()
  }

  fn parse_children(&mut self, until: Option<&str>) -> ArenaVec<'a, VNode<'a, 'b>> {
    let mut children = ArenaVec::new_in(self.parser.vue_allocator);

    while let Some(token) = self.peek() {
      match token.kind {
        VTokenKind::HTMLEndTagOpen => {
          if until.is_some() {
            break;
          }
          self.consume_unmatched_end_tag();
        }
        VTokenKind::HTMLTagOpen => {
          if let Some(node) = self.parse_element() {
            children.push(node);
          }
        }
        VTokenKind::VExpressionStart => {
          if let Some(node) = self.parse_interpolation() {
            children.push(node);
          }
        }
        VTokenKind::HTMLComment | VTokenKind::HTMLBogusComment => {
          let token = self.next().unwrap();
          let value = self.alloc_value(token.value.unwrap_or_default());
          self.parser.sfc.template_comments.push(VComment {
            r#type: token.kind.comment_type(),
            value,
            span: token.span,
          });
        }
        VTokenKind::HTMLText
        | VTokenKind::HTMLWhitespace
        | VTokenKind::HTMLRawText
        | VTokenKind::HTMLRCDataText
        | VTokenKind::HTMLCDataText => {
          let token = self.next().unwrap();
          if let Some(node) = self.text_node(token) {
            children.push(node);
          }
        }
        _ => {
          let token = self.next().unwrap();
          self.parser.sfc.template_tokens.push(token.into());
        }
      }
    }

    children
  }

  fn parse_element(&mut self) -> Option<VNode<'a, 'b>> {
    let open = self.next()?;
    self.parser.sfc.template_tokens.push(open.into());

    let start = self.parse_start_tag(open.span.start)?;
    let is_raw_text = is_raw_text_tag(start.raw_name_source);
    let is_rc_data = is_rc_data_tag(start.raw_name_source);
    let is_script = start.raw_name_source.eq_ignore_ascii_case("script");
    let has_v_pre = start.has_v_pre;

    if has_v_pre {
      self.lexer.enter_v_pre();
    }

    let mut children = if start.self_closing {
      ArenaVec::new_in(self.parser.vue_allocator)
    } else if is_raw_text {
      self.lexer.set_raw_text_mode(start.raw_name_source);
      self.parse_raw_text_children(is_script)
    } else if is_rc_data {
      self.lexer.set_rc_data_mode(start.raw_name_source);
      self.parse_children(Some(start.raw_name_source))
    } else {
      self.parse_children(Some(start.raw_name_source))
    };

    let end_tag =
      if start.self_closing { None } else { self.consume_end_tag(start.raw_name_source) };

    if has_v_pre {
      self.lexer.leave_v_pre();
    }

    let span_end = end_tag.as_ref().map_or(start.span.end, |tag| tag.span.end);
    let element_variables = self.collect_start_tag_variables(&start.attributes);
    let start_tag_variables = self.collect_start_tag_variables(&start.attributes);
    let element = VElement {
      name: start.name,
      raw_name: start.raw_name,
      start_tag: VStartTag {
        attributes: start.attributes,
        variables: start_tag_variables,
        self_closing: start.self_closing,
        span: start.span,
      },
      children: {
        let mut ret = ArenaVec::new_in(self.parser.vue_allocator);
        ret.append(&mut children);
        ret
      },
      end_tag,
      variables: element_variables,
      span: Span::new(open.span.start, span_end),
    };

    Some(VNode::Element(ArenaBox::new_in(element, self.parser.vue_allocator)))
  }

  fn parse_start_tag(&mut self, open_start: u32) -> Option<ParsedStartTag<'a, 'b>> {
    let name_token = self.next_non_ws()?;
    if name_token.kind != VTokenKind::HTMLIdentifier {
      self.lexer.jump_to_eof();
      self.parser.errors.push(error::unexpected_token(name_token.span, "tag name"));
      return None;
    }

    self.parser.sfc.template_tokens.push(name_token.into());
    let mut raw_name_end = name_token.span.end;
    while self
      .peek()
      .is_some_and(|token| token.kind == VTokenKind::Punctuator && token.value == Some("."))
    {
      let dot = self.next().unwrap();
      self.parser.sfc.template_tokens.push(dot.into());
      let Some(part) = self.next() else {
        break;
      };
      if part.kind != VTokenKind::HTMLIdentifier {
        self.peeked = Some(part);
        break;
      }
      raw_name_end = part.span.end;
      self.parser.sfc.template_tokens.push(part.into());
    }
    let raw_name_source =
      &self.parser.source_text[name_token.span.start as usize..raw_name_end as usize];
    let name = self.alloc_value(raw_name_source);
    let raw_name = name;
    let mut attributes = ArenaVec::new_in(self.parser.vue_allocator);
    let mut has_v_pre = false;
    let mut lang = None;
    let mut close_span = name_token.span;
    let mut self_closing = false;

    loop {
      let Some(token) = self.peek() else {
        self.lexer.jump_to_eof();
        self.parser.errors.push(error::unexpected_eof(Span::new(open_start, close_span.end)));
        break;
      };

      match token.kind {
        VTokenKind::HTMLTagClose | VTokenKind::HTMLSelfClosingTagClose => {
          let token = self.next().unwrap();
          close_span = token.span;
          self_closing = token.kind == VTokenKind::HTMLSelfClosingTagClose;
          self.parser.sfc.template_tokens.push(token.into());
          break;
        }
        VTokenKind::HTMLWhitespace => {
          let token = self.next().unwrap();
          self.parser.sfc.template_tokens.push(token.into());
        }
        _ => {
          let attr = self.parse_attribute();
          if attr.name == "v-pre" {
            has_v_pre = true;
          }
          if attr.name == "lang" {
            lang = attr.value;
          }
          if let Some(ast) = attr.ast {
            attributes.push(ast);
          }
        }
      }
    }

    if raw_name_source.eq_ignore_ascii_case("script") {
      self.apply_script_source_type(lang);
    }

    Some(ParsedStartTag {
      name,
      raw_name,
      raw_name_source,
      attributes,
      self_closing,
      has_v_pre,
      span: Span::new(open_start, close_span.end),
    })
  }

  fn collect_start_tag_variables(
    &self,
    attributes: &ArenaVec<'a, VAttribute<'a, 'b>>,
  ) -> ArenaVec<'a, Variable<'a>> {
    let mut variables = ArenaVec::new_in(self.parser.vue_allocator);

    for attribute in attributes {
      match attribute {
        VAttribute::VForDirective(directive) => {
          self.collect_formal_parameter_variables(&directive.value.left, "v-for", &mut variables);
        }
        VAttribute::VSlotDirective(directive) => {
          self.collect_formal_parameter_variables(
            &directive.value.params,
            "v-slot",
            &mut variables,
          );
        }
        VAttribute::VPureAttribute(_) | VAttribute::VDirective(_) | VAttribute::VOnDirective(_) => {
        }
      }
    }

    variables
  }

  fn collect_formal_parameter_variables(
    &self,
    params: &FormalParameters<'b>,
    kind: &'static str,
    variables: &mut ArenaVec<'a, Variable<'a>>,
  ) {
    for param in &params.items {
      self.collect_binding_pattern_variables(&param.pattern, kind, variables);
    }
    if let Some(rest) = &params.rest {
      self.collect_binding_pattern_variables(&rest.rest.argument, kind, variables);
    }
  }

  fn collect_binding_pattern_variables(
    &self,
    pattern: &BindingPattern<'b>,
    kind: &'static str,
    variables: &mut ArenaVec<'a, Variable<'a>>,
  ) {
    match pattern {
      BindingPattern::BindingIdentifier(identifier) => {
        variables.push(Variable {
          name: self.parser.vue_allocator.alloc_str(identifier.name.as_str()),
          span: identifier.span,
          kind,
        });
      }
      BindingPattern::ObjectPattern(pattern) => {
        for property in &pattern.properties {
          self.collect_binding_pattern_variables(&property.value, kind, variables);
        }
        if let Some(rest) = &pattern.rest {
          self.collect_binding_pattern_variables(&rest.argument, kind, variables);
        }
      }
      BindingPattern::ArrayPattern(pattern) => {
        for element in pattern.elements.iter().flatten() {
          self.collect_binding_pattern_variables(element, kind, variables);
        }
        if let Some(rest) = &pattern.rest {
          self.collect_binding_pattern_variables(&rest.argument, kind, variables);
        }
      }
      BindingPattern::AssignmentPattern(pattern) => {
        self.collect_binding_pattern_variables(&pattern.left, kind, variables);
      }
    }
  }

  fn parse_attribute(&mut self) -> ParsedAttribute<'a, 'b> {
    let Some(first) = self.next() else {
      return ParsedAttribute { ast: None, name: "", value: None };
    };

    self.parser.sfc.template_tokens.push(first.into());
    let raw_start = first.span.start;
    let mut raw_end = first.span.end;
    let mut raw_name_end = first.span.end;
    let mut value_span = None;
    let name = if first.kind == VTokenKind::Punctuator
      && let Some(next) = self.next_non_ws()
    {
      self.parser.sfc.template_tokens.push(next.into());
      raw_end = next.span.end;
      raw_name_end = next.span.end;
      &self.parser.source_text[raw_start as usize..raw_name_end as usize]
    } else {
      first.value.unwrap_or_default()
    };

    while let Some(token) = self.peek() {
      if token.kind != VTokenKind::Punctuator {
        break;
      }
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
      && let Some(value_span) = value_span
      && let Some(ast) =
        self.parse_directive_attribute(raw_name, Span::new(raw_start, raw_end), value_span)
    {
      return ParsedAttribute { ast: Some(ast), name, value };
    }

    let key_name = self.alloc_value(name.trim_start_matches([':', '@', '#']));
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
    value_span: Span,
  ) -> Option<VAttribute<'a, 'b>> {
    let (name, argument, modifiers) = self.parse_directive_key(raw_name, attr_span)?;
    let directive_name = name.name;

    if directive_name == "for"
      && let Some(value) = self.parse_v_for_expression(value_span)
    {
      let directive = VForDirective {
        key: VDirectiveKey {
          name,
          argument,
          modifiers,
          span: Span::new(attr_span.start, value_span.start),
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
      && let Some(value) = self.parse_v_slot_expression(value_span)
    {
      let directive = VSlotDirective {
        key: VDirectiveKey {
          name,
          argument,
          modifiers,
          span: Span::new(attr_span.start, value_span.start),
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
      && let Some(value) = self.parse_v_on_expression(value_span)
    {
      let directive = VOnDirective {
        key: VDirectiveKey {
          name,
          argument,
          modifiers,
          span: Span::new(attr_span.start, value_span.start),
        },
        value,
        span: attr_span,
      };

      return Some(VAttribute::VOnDirective(ArenaBox::new_in(
        directive,
        self.parser.vue_allocator,
      )));
    }

    let (expression, references, tokens) = self.parser.parse_pure_expression(value_span)?;
    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }

    let directive = VDirective {
      key: VDirectiveKey {
        name,
        argument,
        modifiers,
        span: Span::new(attr_span.start, value_span.start),
      },
      value: VDirectiveExpression { expression, references, span: value_span },
      span: attr_span,
    };

    Some(VAttribute::VDirective(ArenaBox::new_in(directive, self.parser.vue_allocator)))
  }

  fn parse_v_for_expression(&mut self, value_span: Span) -> Option<VForExpression<'a, 'b>> {
    let source = value_span.source_text(self.parser.source_text);
    let (left_source, right_source, operator_index) = split_v_for_expression(source)?;
    let left_span = trimmed_sub_span(value_span, left_source, source);
    let right_span = Span::new(
      value_span.start
        + operator_index as u32
        + source[operator_index..].find(right_source)? as u32,
      value_span.start
        + operator_index as u32
        + source[operator_index..].find(right_source)? as u32
        + right_source.len() as u32,
    );

    let allocator = Allocator::new();
    let left_trimmed = left_source.trim();
    let mut expression = if left_trimmed.starts_with('(') && left_trimmed.ends_with(')') {
      // SAFETY: this wrapper forms an arrow function with the v-for aliases as params.
      unsafe { self.parser.parse_expression(left_span, b"(", b"=>0)", &allocator)? }.0
    } else {
      // SAFETY: this wrapper forms an arrow function with one v-for alias param.
      unsafe { self.parser.parse_expression(left_span, b"((", b")=>0)", &allocator)? }.0
    };

    let Expression::ArrowFunctionExpression(arrow) = &mut expression else {
      return None;
    };

    let params = arrow.params.clone_in(self.parser.js_allocator);
    let (right, references, tokens) = self.parser.parse_pure_expression(right_span)?;
    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }

    Some(VForExpression { left: params, right, references, span: value_span })
  }

  fn parse_v_slot_expression(&mut self, value_span: Span) -> Option<VSlotExpression<'b>> {
    let allocator = Allocator::new();
    // SAFETY: this wrapper forms an arrow function with slot props as params.
    let mut expression =
      unsafe { self.parser.parse_expression(value_span, b"((", b")=>0)", &allocator)? }.0;
    let Expression::ArrowFunctionExpression(arrow) = &mut expression else {
      return None;
    };

    Some(VSlotExpression {
      params: arrow.params.clone_in(self.parser.js_allocator),
      span: value_span,
    })
  }

  fn parse_v_on_expression(&mut self, value_span: Span) -> Option<VOnExpression<'a, 'b>> {
    let allocator = Allocator::new();
    let ret = self.parser.oxc_parse(value_span, b"{", b"}", Some(&allocator))?;
    let Some(Statement::BlockStatement(block)) = ret.statements.into_iter().next() else {
      return None;
    };

    Some(VOnExpression {
      body: block.body.clone_in(self.parser.js_allocator),
      references: ret.references,
      span: value_span,
    })
  }

  fn parse_directive_key(
    &self,
    raw_name: &'b str,
    attr_span: Span,
  ) -> Option<(&'a VIdentifier<'a>, VDirectiveArgument<'a, 'b>, ArenaVec<'a, VIdentifier<'a>>)> {
    let (directive_name, rest, rest_start) = if let Some(rest) = raw_name.strip_prefix(':') {
      ("bind", rest, attr_span.start + 1)
    } else if let Some(rest) = raw_name.strip_prefix('@') {
      ("on", rest, attr_span.start + 1)
    } else if let Some(rest) = raw_name.strip_prefix('#') {
      ("slot", rest, attr_span.start + 1)
    } else {
      let rest = raw_name.strip_prefix("v-")?;
      let split = rest.find([':', '.']).unwrap_or(rest.len());
      ("", &rest[split..], attr_span.start + 2 + split as u32)
    };

    let directive_name = if directive_name.is_empty() {
      let rest = raw_name.strip_prefix("v-")?;
      let split = rest.find([':', '.']).unwrap_or(rest.len());
      &rest[..split]
    } else {
      directive_name
    };

    let name_start = match raw_name.as_bytes().first() {
      Some(b':' | b'@' | b'#') => attr_span.start,
      _ => attr_span.start + 2,
    };
    let name_span = Span::sized(name_start, directive_name.len() as u32);
    let name = self.parser.vue_allocator.alloc(VIdentifier {
      name: self.alloc_value(directive_name),
      raw_name: self.alloc_value(directive_name),
      span: name_span,
    });

    let mut modifiers = ArenaVec::new_in(self.parser.vue_allocator);
    let (argument_source, modifier_source) = split_directive_argument(rest);
    let argument = argument_source.map_or_else(
      || {
        VDirectiveArgument::VIdentifier(ArenaBox::new_in(
          VIdentifier { name: "", raw_name: "", span: Span::new(rest_start, rest_start) },
          self.parser.vue_allocator,
        ))
      },
      |argument_source| {
        let arg_start = rest_start + rest.find(argument_source).unwrap_or_default() as u32;
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

    for modifier in modifier_source {
      let modifier_start = rest_start + rest.find(modifier).unwrap_or_default() as u32;
      modifiers.push(VIdentifier {
        name: self.alloc_value(modifier),
        raw_name: self.alloc_value(modifier),
        span: Span::sized(modifier_start, modifier.len() as u32),
      });
    }

    Some((name, argument, modifiers))
  }

  fn parse_raw_text_children(&mut self, is_script: bool) -> ArenaVec<'a, VNode<'a, 'b>> {
    let mut children = ArenaVec::new_in(self.parser.vue_allocator);
    let mut raw_start = None;
    let mut raw_end = None;

    while let Some(token) = self.peek() {
      if token.kind == VTokenKind::HTMLEndTagOpen {
        break;
      }

      let token = self.next().unwrap();
      self.parser.sfc.template_tokens.push(token.into());
      raw_start.get_or_insert(token.span.start);
      raw_end = Some(token.span.end);
    }

    let Some(start) = raw_start else {
      return children;
    };
    let span = Span::new(start, raw_end.unwrap_or(start));

    if is_script && let Some(script) = self.parse_script(span) {
      children.push(script);
      return children;
    }

    let text = self.alloc_value(span.source_text(self.parser.source_text));
    children.push(VNode::Text(ArenaBox::new_in(VText { text, span }, self.parser.vue_allocator)));
    children
  }

  fn parse_script(&mut self, span: Span) -> Option<VNode<'a, 'b>> {
    if span.source_text(self.parser.source_text).trim().is_empty() {
      return None;
    }

    let ret = self.parser.oxc_parse(span, &[], &[], Some(self.parser.js_allocator))?;

    for directive in &ret.directives {
      self.parser.clean_spans.insert(directive.span());
    }
    for statement in &ret.statements {
      self.parser.clean_spans.insert(statement.span());
    }

    merge_module_record(&mut self.parser.module_record, ret.module_record);
    if !ret.tokens.is_empty() {
      self.parser.sfc.script_tokens.push(ret.tokens.into());
    }

    let script = VPureScript { statements: ret.statements, directives: ret.directives, span };
    Some(VNode::PureScript(ArenaBox::new_in(script, self.parser.vue_allocator)))
  }

  fn parse_interpolation(&mut self) -> Option<VNode<'a, 'b>> {
    let start = self.next()?;
    self.parser.sfc.template_tokens.push(start.into());
    let expression = self.next()?;
    self.parser.sfc.template_tokens.push(expression.into());
    let end = self.next()?;
    self.parser.sfc.template_tokens.push(end.into());

    if expression.kind != VTokenKind::HTMLText || end.kind != VTokenKind::VExpressionEnd {
      self.parser.errors.push(error::unexpected_token(expression.span, "interpolation expression"));
      return None;
    }

    let span = Span::new(expression.span.start, expression.span.end);
    let Some((expression, references, tokens)) = self.parser.parse_pure_expression(span) else {
      return self.text_node(VToken::new(
        VTokenKind::HTMLText,
        Span::new(start.span.start, end.span.end),
        Some(&self.parser.source_text[start.span.start as usize..end.span.end as usize]),
      ));
    };

    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }
    let interpolation = crate::ast::VInterpolation {
      expression,
      references,
      span: Span::new(start.span.start, end.span.end),
    };
    Some(VNode::Interpolation(ArenaBox::new_in(interpolation, self.parser.vue_allocator)))
  }

  fn consume_end_tag(&mut self, name: &str) -> Option<VEndTag> {
    if !self.peek().is_some_and(|token| token.kind == VTokenKind::HTMLEndTagOpen) {
      return None;
    }

    let open = self.next().unwrap();
    self.parser.sfc.template_tokens.push(open.into());
    let name_token = self.next_non_ws()?;
    self.parser.sfc.template_tokens.push(name_token.into());
    if !name_token.value.unwrap_or_default().eq_ignore_ascii_case(name) {
      self.parser.errors.push(error::unexpected_closing_tag(name_token.span));
    }

    let mut end = name_token.span.end;
    while let Some(token) = self.next() {
      end = token.span.end;
      let should_break = token.kind == VTokenKind::HTMLTagClose;
      self.parser.sfc.template_tokens.push(token.into());
      if should_break {
        break;
      }
    }

    Some(VEndTag { span: Span::new(open.span.start, end) })
  }

  fn consume_unmatched_end_tag(&mut self) {
    let start = self.next().unwrap();
    self.parser.sfc.template_tokens.push(start.into());
    while let Some(token) = self.next() {
      let should_break = token.kind == VTokenKind::HTMLTagClose;
      self.parser.sfc.template_tokens.push(token.into());
      if should_break {
        break;
      }
    }
  }

  fn text_node(&mut self, token: VToken<'b>) -> Option<VNode<'a, 'b>> {
    self.parser.sfc.template_tokens.push(token.into());
    let text = token.value?;
    let text = self.alloc_value(text);
    Some(VNode::Text(ArenaBox::new_in(VText { text, span: token.span }, self.parser.vue_allocator)))
  }

  fn apply_script_source_type(&mut self, lang: Option<&str>) {
    let lang = lang.unwrap_or("js");
    match SourceType::from_extension(lang) {
      Ok(source_type) => self.parser.sfc.source_type = Some(source_type),
      Err(_) => self.parser.errors.push(error::unexpected_script_lang(lang)),
    }
  }

  fn next_non_ws(&mut self) -> Option<VToken<'b>> {
    loop {
      let token = self.next()?;
      if token.kind != VTokenKind::HTMLWhitespace {
        return Some(token);
      }
      self.parser.sfc.template_tokens.push(token.into());
    }
  }

  fn peek(&mut self) -> Option<VToken<'b>> {
    if self.peeked.is_none() {
      self.peeked = self.lexer.next_token();
    }
    self.peeked
  }

  fn next(&mut self) -> Option<VToken<'b>> {
    self.peeked.take().or_else(|| self.lexer.next_token())
  }

  fn alloc_value(&self, value: &str) -> &'a str {
    self.parser.vue_allocator.alloc_str(value)
  }
}

trait CommentToken {
  fn comment_type(self) -> &'static str;
}

impl CommentToken for VTokenKind {
  fn comment_type(self) -> &'static str {
    match self {
      Self::HTMLBogusComment => "HTMLBogusComment",
      _ => "HTMLComment",
    }
  }
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

fn is_directive_name(name: &str) -> bool {
  name.starts_with("v-") || name.starts_with(':') || name.starts_with('@') || name.starts_with('#')
}

fn split_directive_argument(rest: &str) -> (Option<&str>, Vec<&str>) {
  let rest = rest.strip_prefix(':').unwrap_or(rest);
  let mut parts = rest.split('.');
  let argument = parts.next().filter(|argument| !argument.is_empty());
  (argument, parts.filter(|modifier| !modifier.is_empty()).collect())
}

fn split_v_for_expression(source: &str) -> Option<(&str, &str, usize)> {
  for operator in [" in ", " of "] {
    if let Some(index) = source.find(operator) {
      return Some((&source[..index], &source[index + operator.len()..], index));
    }
  }

  None
}

fn trimmed_sub_span(parent: Span, child: &str, parent_source: &str) -> Span {
  let leading = child.len() - child.trim_start().len();
  let len = child.trim().len();
  let start = parent_source.find(child).unwrap_or_default() + leading;
  Span::new(parent.start + start as u32, parent.start + start as u32 + len as u32)
}

fn is_raw_text_tag(name: &str) -> bool {
  matches!(
    name.to_ascii_lowercase().as_str(),
    "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript" | "plaintext"
  )
}

fn is_rc_data_tag(name: &str) -> bool {
  matches!(name.to_ascii_lowercase().as_str(), "textarea" | "title")
}
