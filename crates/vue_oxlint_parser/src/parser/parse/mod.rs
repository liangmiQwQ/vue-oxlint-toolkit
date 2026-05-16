mod attributes;
mod directives;
mod elements;
mod emit;
mod state;

use crate::lexer::{Lexer, VToken, VTokenKind};
use crate::parser::irregular_whitespaces::collect_irregular_whitespaces;
use crate::{VueParser, VueParserReturn};
use oxc_span::SourceType;

impl<'a, 'b> VueParser<'a, 'b> {
  #[must_use]
  pub fn parse(mut self) -> VueParserReturn<'a, 'b> {
    self.parse_tokens();

    let Self { sfc, errors, clean_spans, module_record, source_text, .. } = self;

    VueParserReturn {
      sfc,
      irregular_whitespaces: collect_irregular_whitespaces(source_text),
      module_record,
      clean_spans,
      errors,
      panicked: false,
    }
  }
}

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  fn parse_tokens(&mut self) {
    self.sfc.source_type = Some(SourceType::mjs());

    let mut lexer = Lexer::new(self.js_allocator, self.source_text);
    let mut current_tag = None;
    let mut element_stack = Vec::new();
    let mut raw_element = None;
    let mut pending_script = None;
    let mut interpolation_start = None;
    let mut seen_script = false;
    let mut seen_setup = false;

    while let Some(token) = lexer.next_token() {
      if let Some(start) = interpolation_start {
        if token.kind == VTokenKind::VExpressionEnd {
          self.emit_expression_tokens(start, token_start(token));
          self.push_template_vtoken(token);
          interpolation_start = None;
        }
        continue;
      }

      match token.kind {
        VTokenKind::HTMLComment | VTokenKind::HTMLBogusComment => {
          self.push_template_comment(token);
        }
        VTokenKind::HTMLTagOpen | VTokenKind::HTMLEndTagOpen => {
          self.handle_tag_open(token, &mut current_tag, &mut raw_element, &mut pending_script);
        }
        VTokenKind::HTMLTagClose | VTokenKind::HTMLSelfClosingTagClose => {
          if let Some(tag) = &mut current_tag {
            self.flush_attr_name(tag);
            self.flush_attr_value(tag);
            Self::analyze_tag_attrs(tag);
            self.emit_tag_attrs(tag);
          }
          self.push_template_vtoken(token);
          self.finish_tag(
            token,
            &mut lexer,
            &mut current_tag,
            &mut element_stack,
            &mut raw_element,
            &mut pending_script,
            &mut seen_script,
            &mut seen_setup,
          );
        }
        VTokenKind::HTMLIdentifier => {
          self.handle_identifier(token, &mut current_tag);
        }
        VTokenKind::Punctuator if current_tag.as_ref().is_some_and(|tag| !tag.is_end) => {
          self.handle_attr_name_part(token, &mut current_tag);
        }
        VTokenKind::HTMLAssociation => {
          if let Some(tag) = &mut current_tag
            && tag.awaiting_attr_value.is_none()
          {
            self.start_attr_value(tag, token);
          } else {
            self.handle_attr_name_part(token, &mut current_tag);
          }
        }
        VTokenKind::HTMLLiteral => {
          self.handle_literal(token, &mut current_tag);
        }
        VTokenKind::VExpressionStart => {
          self.push_template_vtoken(token);
          interpolation_start = Some(token_end(token));
        }
        VTokenKind::HTMLWhitespace if current_tag.is_some() => {
          if let Some(tag) = &mut current_tag {
            if tag.awaiting_attr_value.is_some() {
              self.flush_attr_value(tag);
            } else {
              self.flush_attr_name(tag);
            }
          }
        }
        _ => self.push_template_vtoken(token),
      }
    }
  }

  pub(super) fn alloc_str(&self, value: &str) -> &'b str {
    self.js_allocator.alloc_str(value)
  }

  pub(super) fn skip_ws(&self, mut pos: usize, end: usize) -> usize {
    while pos < end && self.byte(pos).is_ascii_whitespace() {
      pos += 1;
    }
    pos
  }

  pub(super) fn trim_end_ws(&self, start: usize, mut end: usize) -> usize {
    while end > start && self.byte(end - 1).is_ascii_whitespace() {
      end -= 1;
    }
    end
  }

  pub(super) fn byte(&self, pos: usize) -> u8 {
    self.source_text.as_bytes()[pos]
  }
}

pub(super) const fn token_start(token: VToken<'_>) -> usize {
  token.span.start as usize
}

pub(super) const fn token_end(token: VToken<'_>) -> usize {
  token.span.end as usize
}
