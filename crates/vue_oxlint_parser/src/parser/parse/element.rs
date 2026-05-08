use oxc_allocator::{Box as ArenaBox, Vec as ArenaVec};
use oxc_span::{SourceType, Span};

use crate::ast::token::SerializableToken;
use crate::ast::{VAttribute, VElement, VNode, VStartTag};
use crate::lexer::{VToken, VTokenKind};
use crate::parser::parse::utils::{is_raw_text_tag, is_rc_data_tag, is_void_tag};
use crate::{error, parser::parse::TemplateParser};

struct ParsedStartTag<'a, 'b> {
  name: &'a str,
  raw_name: &'a str,
  raw_name_source: &'b str,
  attributes: ArenaVec<'a, VAttribute<'a, 'b>>,
  self_closing: bool,
  has_v_pre: bool,
  span: Span,
}

impl<'a, 'b> TemplateParser<'_, 'a, 'b>
where
  'b: 'a,
{
  pub(super) fn parse_element(&mut self) -> Option<VNode<'a, 'b>> {
    let open = self.next()?;
    self.parser.sfc.template_tokens.push(open.into());

    let start = self.parse_start_tag(open)?;
    let is_raw_text = is_raw_text_tag(start.raw_name_source);
    let is_rc_data = is_rc_data_tag(start.raw_name_source);
    let is_script = start.raw_name_source.eq_ignore_ascii_case("script");
    let is_void = is_void_tag(start.raw_name_source);
    let has_v_pre = start.has_v_pre;

    if has_v_pre {
      self.lexer.enter_v_pre();
    }
    if is_script {
      self.parser.sfc.script_tokens.push(SerializableToken::script_tag(start.span));
    }

    let mut children = if start.self_closing || is_void {
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

    let end_tag = if start.self_closing || is_void {
      None
    } else {
      self.consume_end_tag(start.raw_name_source)
    };

    if has_v_pre {
      self.lexer.leave_v_pre();
    }
    if is_script && let Some(end_tag) = end_tag.as_ref() {
      self.parser.sfc.script_tokens.push(SerializableToken::script_end_tag(end_tag.span));
    }

    let span_end = end_tag.as_ref().map_or(start.span.end, |tag| tag.span.end);
    let variables = self.collect_start_tag_variables(&start.attributes);
    let start_tag_variables = self.clone_variables(&variables);
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
      variables,
      span: Span::new(open.span.start, span_end),
    };

    Some(VNode::Element(ArenaBox::new_in(element, self.parser.vue_allocator)))
  }

  fn parse_start_tag(&mut self, open: VToken<'b>) -> Option<ParsedStartTag<'a, 'b>> {
    if open.value.is_empty() {
      self.lexer.jump_to_eof();
      self.parser.errors.push(error::unexpected_token(open.span, "tag name"));
      return None;
    }

    let open_start = open.span.start;
    let name_start = open.span.start + 1;
    let mut raw_name_end = open.span.end;
    while self
      .peek()
      .is_some_and(|token| token.kind == VTokenKind::Punctuator && token.value == ".")
    {
      // SAFETY: `peek()` proved the token exists and is the dot punctuator.
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
    let raw_name_source = &self.parser.source_text[name_start as usize..raw_name_end as usize];
    let name = self.alloc_value(&raw_name_source.to_ascii_lowercase());
    let raw_name = self.alloc_value(raw_name_source);
    let mut attributes = ArenaVec::new_in(self.parser.vue_allocator);
    let mut has_v_pre = false;
    let mut lang = None;
    let mut close_span = open.span;
    let mut self_closing = false;

    loop {
      let Some(token) = self.peek() else {
        self.lexer.jump_to_eof();
        self.parser.errors.push(error::unexpected_eof(Span::new(open_start, close_span.end)));
        break;
      };

      match token.kind {
        VTokenKind::HTMLTagClose | VTokenKind::HTMLSelfClosingTagClose => {
          // SAFETY: the match arm is only reached for a close token returned by `peek()`.
          let token = self.next().unwrap();
          close_span = token.span;
          self_closing = token.kind == VTokenKind::HTMLSelfClosingTagClose;
          self.parser.sfc.template_tokens.push(token.into());
          break;
        }
        VTokenKind::HTMLWhitespace => {
          // SAFETY: the match arm is only reached for a whitespace token returned by `peek()`.
          _ = self.next();
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

  fn apply_script_source_type(&mut self, lang: Option<&str>) {
    let lang = lang.unwrap_or("js");
    match SourceType::from_extension(lang) {
      Ok(source_type) => self.parser.sfc.source_type = Some(source_type),
      Err(_) => self.parser.errors.push(error::unexpected_script_lang(lang)),
    }
  }
}
