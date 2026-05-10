mod irregular_whitespaces;
mod module_record;
mod oxc_parse;

use crate::ast::VComment;
use crate::ast::token::SerializableToken;
use crate::lexer::{VToken, VTokenKind};
use crate::parser::irregular_whitespaces::collect_irregular_whitespaces;
use crate::{VueParser, VueParserReturn};
use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, SourceType, Span};

#[derive(Debug, Clone, Copy)]
struct TagInfo {
  name_start: usize,
  name_end: usize,
  tag_end: usize,
  is_end: bool,
  is_self_closing: bool,
}

#[derive(Debug, Clone, Copy)]
enum AttrValueKind {
  Literal,
  Expression,
  Handler,
  SlotParams,
  VFor,
}

#[derive(Debug, Clone, Copy)]
struct VForParts<'s> {
  left_start: usize,
  left_end: usize,
  operator: &'s str,
  operator_start: usize,
  operator_end: usize,
  right_start: usize,
  right_end: usize,
}

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

    let mut pos = 0;
    let len = self.source_text.len();
    let mut seen_script = false;
    let mut seen_setup = false;
    let mut element_stack = Vec::new();

    while pos < len {
      if self.starts_with(pos, "<!--") {
        pos = self.emit_html_comment(pos);
        continue;
      }

      if self.starts_with(pos, "<![CDATA[") {
        if is_foreign_content(&element_stack) {
          pos = self.emit_cdata_text(pos);
        } else {
          pos = self.emit_bogus_comment(pos);
        }
        continue;
      }

      if self.starts_with(pos, "<!") {
        pos = self.emit_bogus_comment(pos);
        continue;
      }

      if self.byte(pos) == b'<'
        && let Some(tag) = self.read_tag(pos)
      {
        self.emit_tag(&tag);
        pos = tag.tag_end + 1;

        let name = self.lower_source(tag.name_start, tag.name_end);

        if tag.is_end {
          pop_element(&mut element_stack, &name);
        } else if !tag.is_self_closing {
          if is_raw_text_element(&name) {
            pos = self.parse_raw_element_body(pos, &name, &mut seen_script, &mut seen_setup);
          } else if is_rcdata_element(&name) {
            pos = self.parse_rcdata_element_body(pos, &name);
          }
          element_stack.push(name);
        }
        continue;
      }

      let next = self.find_from(pos + 1, "<").unwrap_or(len);
      self.emit_data(pos, next);
      pos = next;
    }
  }

  fn parse_rcdata_element_body(&mut self, body_start: usize, name: &str) -> usize {
    let close = format!("</{name}");
    let Some(close_start) = self.find_ascii_case_insensitive(body_start, &close) else {
      self.emit_rcdata_text_chunks(body_start, self.source_text.len());
      return self.source_text.len();
    };

    self.emit_rcdata_text_chunks(body_start, close_start);
    close_start
  }

  fn parse_raw_element_body(
    &mut self,
    body_start: usize,
    name: &str,
    seen_script: &mut bool,
    seen_setup: &mut bool,
  ) -> usize {
    let close = format!("</{name}");
    let Some(close_start) = self.find_ascii_case_insensitive(body_start, &close) else {
      self.emit_raw_text_chunks(body_start, self.source_text.len());
      return self.source_text.len();
    };

    self.emit_raw_text_chunks(body_start, close_start);

    if name == "script" {
      self.emit_script_tokens(body_start, close_start, seen_script, seen_setup);
    }

    close_start
  }

  fn emit_script_tokens(
    &mut self,
    body_start: usize,
    body_end: usize,
    seen_script: &mut bool,
    seen_setup: &mut bool,
  ) {
    let Some(open_start) = self.source_text[..body_start].rfind("<script") else {
      return;
    };
    let open_end = body_start.saturating_sub(1);
    let is_setup = self.source_text[open_start..open_end].contains("setup");
    if is_setup {
      if *seen_setup || body_start == body_end {
        *seen_setup = true;
        return;
      }
      *seen_setup = true;
    } else {
      if *seen_script || (*seen_setup && body_start == body_end) {
        return;
      }
      *seen_script = true;
    }

    self.push_script_punctuator(open_start, open_end + 1, "<script>");

    self.apply_script_source_type(open_start, open_end + 1);

    if body_start < body_end {
      let span = Span::new(body_start as u32, body_end as u32);
      if let Some((_, _, _, tokens)) = self.oxc_parse(span, &[], &[], None)
        && !tokens.is_empty()
      {
        self.sfc.script_tokens.push(tokens.into());
      }
    }

    let close_end = self.find_from(body_end, ">").map_or(body_end, |end| end + 1);
    self.push_script_punctuator(body_end, close_end, "</script>");
  }

  fn apply_script_source_type(&mut self, open_start: usize, open_end: usize) {
    let mut source_type = SourceType::mjs();
    let attrs = &self.source_text[open_start..open_end];
    if let Some(lang) = attr_value(attrs, "lang")
      && let Ok(parsed) = SourceType::from_extension(lang)
    {
      source_type = parsed.with_module(true);
    }
    self.sfc.source_type = Some(source_type);
  }

  fn emit_tag(&mut self, tag: &TagInfo) {
    let name = self.lower_source(tag.name_start, tag.name_end);
    let tag_start = if tag.is_end { tag.name_start - 2 } else { tag.name_start - 1 };
    let kind = if tag.is_end { VTokenKind::HTMLEndTagOpen } else { VTokenKind::HTMLTagOpen };
    self.push_template_token(kind, tag_start, tag.name_end, Some(self.alloc_str(&name)));

    if !tag.is_end {
      self.emit_attrs(tag.name_end, tag);
    }

    let close_start = if tag.is_self_closing {
      let mut i = tag.tag_end;
      while i > tag.name_end && self.byte(i - 1).is_ascii_whitespace() {
        i -= 1;
      }
      i - 1
    } else {
      tag.tag_end
    };
    let close_kind = if tag.is_self_closing {
      VTokenKind::HTMLSelfClosingTagClose
    } else {
      VTokenKind::HTMLTagClose
    };
    self.push_template_token(close_kind, close_start, tag.tag_end + 1, Some(""));
  }

  fn emit_attrs(&mut self, mut pos: usize, tag: &TagInfo) {
    while pos < tag.tag_end {
      pos = self.skip_ws(pos, tag.tag_end);
      if pos >= tag.tag_end || self.byte(pos) == b'/' {
        break;
      }

      let name_start = pos;
      while pos < tag.tag_end {
        let b = self.byte(pos);
        if b.is_ascii_whitespace() || matches!(b, b'=' | b'/' | b'>') {
          break;
        }
        pos += 1;
      }
      let name_end = pos;
      if name_start == name_end {
        pos += 1;
        continue;
      }

      let attr_name = &self.source_text[name_start..name_end];
      self.emit_attr_name(attr_name, name_start, name_end);

      pos = self.skip_ws(pos, tag.tag_end);
      if pos >= tag.tag_end || self.byte(pos) != b'=' {
        continue;
      }

      self.push_template_token(VTokenKind::HTMLAssociation, pos, pos + 1, Some(""));
      pos += 1;
      pos = self.skip_ws(pos, tag.tag_end);
      if pos >= tag.tag_end {
        break;
      }

      let quote = self.byte(pos);
      if matches!(quote, b'\'' | b'"') {
        let value_start = pos + 1;
        let value_end = self.find_byte(value_start, tag.tag_end, quote).unwrap_or(tag.tag_end);
        self.emit_attr_value(attr_name, value_start, value_end, pos, value_end + 1);
        pos = value_end + 1;
      } else {
        let value_start = pos;
        while pos < tag.tag_end && !self.byte(pos).is_ascii_whitespace() {
          pos += 1;
        }
        self.emit_attr_value(attr_name, value_start, pos, value_start, pos);
      }
    }
  }

  fn emit_attr_name(&mut self, name: &'b str, start: usize, end: usize) {
    if let Some(first) = name.as_bytes().first().copied()
      && matches!(first, b':' | b'@' | b'#')
      && name.len() > 1
    {
      self.push_template_token(VTokenKind::Punctuator, start, start + 1, Some(&name[..1]));
      self.emit_dynamic_arg_tokens(start + 1, end);
      return;
    }

    if let Some(colon) = name.find(':').filter(|_| !name.ends_with(':')) {
      let colon = start + colon;
      self.push_template_token(
        VTokenKind::HTMLIdentifier,
        start,
        colon,
        Some(&self.source_text[start..colon]),
      );
      self.push_template_token(VTokenKind::Punctuator, colon, colon + 1, Some(":"));
      self.emit_dynamic_arg_tokens(colon + 1, end);
      return;
    }

    if let Some(dot) = name.find('.') {
      let dot = start + dot;
      self.push_template_token(
        VTokenKind::HTMLIdentifier,
        start,
        dot,
        Some(&self.source_text[start..dot]),
      );
      self.push_template_token(VTokenKind::Punctuator, dot, dot + 1, Some("."));
      self.push_template_token(
        VTokenKind::HTMLIdentifier,
        dot + 1,
        end,
        Some(&self.source_text[dot + 1..end]),
      );
      return;
    }

    self.push_template_token(VTokenKind::HTMLIdentifier, start, end, Some(name));
  }

  fn emit_dynamic_arg_tokens(&mut self, start: usize, end: usize) {
    if start < end && self.byte(start) == b'[' && self.byte(end - 1) == b']' {
      self.push_template_token(VTokenKind::Punctuator, start, start + 1, Some("["));
      self.emit_expression_tokens(start + 1, end - 1);
      self.push_template_token(VTokenKind::Punctuator, end - 1, end, Some("]"));
    } else if start < end {
      if let Some(dot) = self.source_text[start..end].find('.') {
        let dot = start + dot;
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          start,
          dot,
          Some(&self.source_text[start..dot]),
        );
        self.push_template_token(VTokenKind::Punctuator, dot, dot + 1, Some("."));
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          dot + 1,
          end,
          Some(&self.source_text[dot + 1..end]),
        );
      } else {
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          start,
          end,
          Some(&self.source_text[start..end]),
        );
      }
    }
  }

  fn emit_attr_value(
    &mut self,
    attr_name: &'b str,
    value_start: usize,
    value_end: usize,
    token_start: usize,
    token_end: usize,
  ) {
    let kind = attr_value_kind(attr_name);
    if matches!(kind, AttrValueKind::Literal) {
      self.push_template_token(
        VTokenKind::HTMLLiteral,
        token_start,
        token_end,
        Some(&self.source_text[value_start..value_end]),
      );
    } else {
      if token_start < value_start {
        let quote = &self.source_text[token_start..value_start];
        self.push_template_token(VTokenKind::Punctuator, token_start, value_start, Some(quote));
      }
      match kind {
        AttrValueKind::Expression => self.emit_expression_tokens(value_start, value_end),
        AttrValueKind::Handler => self.emit_handler_tokens(value_start, value_end),
        AttrValueKind::SlotParams => self.emit_slot_params_tokens(value_start, value_end),
        AttrValueKind::VFor => self.emit_v_for_tokens(value_start, value_end),
        AttrValueKind::Literal => {}
      }
      if value_end < token_end {
        let quote = &self.source_text[value_end..token_end];
        self.push_template_token(VTokenKind::Punctuator, value_end, token_end, Some(quote));
      }
    }
  }

  fn emit_data(&mut self, start: usize, end: usize) {
    let mut pos = start;
    while pos < end {
      if self.starts_with(pos, "{{")
        && let Some(close) = self.find_from(pos + 2, "}}")
      {
        self.push_template_token(VTokenKind::VExpressionStart, pos, pos + 2, Some("{{"));
        self.emit_expression_tokens(pos + 2, close);
        self.push_template_token(VTokenKind::VExpressionEnd, close, close + 2, Some("}}"));
        pos = close + 2;
        continue;
      }

      let next_expr = self.find_from(pos + 1, "{{").unwrap_or(end).min(end);
      self.emit_text_chunks(pos, next_expr);
      pos = next_expr;
    }
  }

  fn emit_text_chunks(&mut self, start: usize, end: usize) {
    let mut pos = start;
    while pos < end {
      let chunk_start = pos;
      let is_ws = self.byte(pos).is_ascii_whitespace();
      while pos < end && self.byte(pos).is_ascii_whitespace() == is_ws {
        pos += 1;
      }
      let kind = if is_ws { VTokenKind::HTMLWhitespace } else { VTokenKind::HTMLText };
      self.push_template_token(kind, chunk_start, pos, Some(&self.source_text[chunk_start..pos]));
    }
  }

  fn emit_rcdata_text_chunks(&mut self, start: usize, end: usize) {
    let mut pos = start;
    while pos < end {
      let chunk_start = pos;
      if self.byte(pos).is_ascii_whitespace() {
        while pos < end && self.byte(pos).is_ascii_whitespace() {
          pos += 1;
        }
        self.push_template_token(
          VTokenKind::HTMLWhitespace,
          chunk_start,
          pos,
          Some(&self.source_text[chunk_start..pos]),
        );
        continue;
      }

      if self.byte(pos) == b'&'
        && let Some((reference_end, value)) = self.read_character_reference(pos, end)
      {
        self.push_template_token(
          VTokenKind::HTMLRCDataText,
          chunk_start,
          reference_end,
          Some(value),
        );
        pos = reference_end;
        continue;
      }

      while pos < end && !self.byte(pos).is_ascii_whitespace() && self.byte(pos) != b'&' {
        pos += 1;
      }
      self.push_template_token(
        VTokenKind::HTMLRCDataText,
        chunk_start,
        pos,
        Some(&self.source_text[chunk_start..pos]),
      );
    }
  }

  fn emit_raw_text_chunks(&mut self, start: usize, end: usize) {
    let mut pos = start;
    while pos < end {
      let chunk_start = pos;
      let is_ws = self.byte(pos).is_ascii_whitespace();
      while pos < end && self.byte(pos).is_ascii_whitespace() == is_ws {
        pos += 1;
      }
      let kind = if is_ws { VTokenKind::HTMLWhitespace } else { VTokenKind::HTMLRawText };
      self.push_template_token(kind, chunk_start, pos, Some(&self.source_text[chunk_start..pos]));
    }
  }

  fn emit_html_comment(&mut self, start: usize) -> usize {
    let (end, value_end) = if let Some(end) = self.find_from(start + 4, "-->") {
      (end + 3, end)
    } else {
      (self.source_text.len(), self.source_text.len())
    };
    self.sfc.template_comments.push(VComment {
      r#type: "HTMLComment",
      value: &self.source_text[start + 4..value_end],
      span: Span::new(start as u32, end as u32),
    });
    end
  }

  fn emit_bogus_comment(&mut self, start: usize) -> usize {
    let (end, value_end) = if let Some(end) = self.find_from(start + 2, ">") {
      (end + 1, end)
    } else {
      (self.source_text.len(), self.source_text.len())
    };
    self.sfc.template_comments.push(VComment {
      r#type: "HTMLBogusComment",
      value: &self.source_text[start + 2..value_end],
      span: Span::new(start as u32, end as u32),
    });
    end
  }

  fn emit_cdata_text(&mut self, start: usize) -> usize {
    let value_start = start + "<![CDATA[".len();
    let (end, value_end) = if let Some(end) = self.find_from(value_start, "]]>") {
      (end + 3, end)
    } else {
      (self.source_text.len(), self.source_text.len())
    };
    self.push_template_token(
      VTokenKind::HTMLCDataText,
      start,
      end,
      Some(&self.source_text[value_start..value_end]),
    );
    end
  }

  fn emit_expression_tokens(&mut self, start: usize, end: usize) {
    let span = Span::new(start as u32, end as u32);
    if let Some((_, tokens)) = self.parse_pure_expression(span)
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  fn emit_handler_tokens(&mut self, start: usize, end: usize) {
    let span = Span::new(start as u32, end as u32);
    let allocator = Allocator::new();
    let tokens = unsafe {
      self.parse_expression(span, b"(()=>{", b"})", &allocator, |expression| {
        let Expression::ArrowFunctionExpression(arrow) = expression else {
          return None;
        };

        let Some(first) = arrow.body.statements.first() else {
          return Some(((), Span::new(span.start, span.start)));
        };
        let last = arrow.body.statements.last().unwrap_or(first);

        Some(((), Span::new(first.span().start, last.span().end)))
      })
    };

    if let Some(((), tokens)) = tokens
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  fn emit_slot_params_tokens(&mut self, start: usize, end: usize) {
    let start = self.skip_ws(start, end);
    let end = self.trim_end_ws(start, end);
    if start >= end {
      return;
    }

    let span = Span::new(start as u32, end as u32);
    let is_parenthesized = self.byte(start) == b'(' && self.byte(end - 1) == b')';
    let (start_wrap, end_wrap): (&[u8], &[u8]) =
      if is_parenthesized { (b"(", b"=>0)") } else { (b"((", b")=>0)") };

    let allocator = Allocator::new();
    let tokens = unsafe {
      self.parse_expression(span, start_wrap, end_wrap, &allocator, |expression| {
        let Expression::ArrowFunctionExpression(arrow) = expression else {
          return None;
        };

        let token_span = if is_parenthesized {
          span
        } else if let Some(first) = arrow.params.items.first() {
          let end = arrow.params.rest.as_ref().map_or_else(
            || arrow.params.items.last().map_or(first.span.end, |last| last.span.end),
            |rest| rest.span.end,
          );
          Span::new(first.span.start, end)
        } else if let Some(rest) = &arrow.params.rest {
          rest.span
        } else {
          Span::new(span.start, span.start)
        };

        Some(((), token_span))
      })
    };

    if let Some(((), tokens)) = tokens
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  fn emit_v_for_tokens(&mut self, start: usize, end: usize) {
    let Some(parts) = self.split_v_for_expression(start, end) else {
      self.emit_expression_tokens(start, end);
      return;
    };

    self.emit_slot_params_tokens(parts.left_start, parts.left_end);
    self.push_manual_oxc_token(
      if parts.operator == "in" { "Keyword" } else { "Identifier" },
      parts.operator,
      parts.operator_start,
      parts.operator_end,
    );
    self.emit_expression_tokens(parts.right_start, parts.right_end);
  }

  fn push_template_oxc_tokens(&mut self, tokens: &'a str) {
    if !tokens.is_empty() {
      self.sfc.template_tokens.push(tokens.into());
    }
  }

  fn push_manual_oxc_token(&mut self, token_type: &str, value: &str, start: usize, end: usize) {
    let token =
      format!(r#"{{"type":"{token_type}","value":"{value}","start":{start},"end":{end}}}"#);
    let token = self.vue_allocator.alloc_str(&token);
    self.sfc.template_tokens.push(token.into());
  }

  fn split_v_for_expression(&self, start: usize, end: usize) -> Option<VForParts<'b>> {
    let mut pos = start;
    let mut paren_depth = 0_u32;
    let mut bracket_depth = 0_u32;
    let mut brace_depth = 0_u32;

    while pos < end {
      match self.byte(pos) {
        b'\'' | b'"' => {
          pos = self.skip_quoted(pos, end);
          continue;
        }
        b'`' => {
          pos = self.skip_template_literal(pos, end);
          continue;
        }
        b'(' => paren_depth += 1,
        b')' => paren_depth = paren_depth.saturating_sub(1),
        b'[' => bracket_depth += 1,
        b']' => bracket_depth = bracket_depth.saturating_sub(1),
        b'{' => brace_depth += 1,
        b'}' => brace_depth = brace_depth.saturating_sub(1),
        byte
          if byte.is_ascii_whitespace()
            && paren_depth == 0
            && bracket_depth == 0
            && brace_depth == 0 =>
        {
          let operator_start = self.skip_ws(pos, end);
          let operator_source = &self.source_text[operator_start..end];
          let operator_end =
            if operator_source.starts_with("in") || operator_source.starts_with("of") {
              operator_start + 2
            } else {
              pos += 1;
              continue;
            };

          if operator_end >= end || !self.byte(operator_end).is_ascii_whitespace() {
            pos += 1;
            continue;
          }

          let left_start = self.skip_ws(start, pos);
          let left_end = self.trim_end_ws(left_start, pos);
          let right_start = self.skip_ws(operator_end, end);
          let right_end = self.trim_end_ws(right_start, end);
          if left_start >= left_end || right_start >= right_end {
            return None;
          }

          return Some(VForParts {
            left_start,
            left_end,
            operator: &self.source_text[operator_start..operator_end],
            operator_start,
            operator_end,
            right_start,
            right_end,
          });
        }
        _ => {}
      }
      pos += 1;
    }

    None
  }

  fn push_script_punctuator(&mut self, start: usize, end: usize, value: &'static str) {
    self.sfc.script_tokens.push(
      VToken::new(VTokenKind::Punctuator, Span::new(start as u32, end as u32), Some(value)).into(),
    );
  }

  fn push_template_token(
    &mut self,
    kind: VTokenKind,
    start: usize,
    end: usize,
    value: Option<&'b str>,
  ) {
    self.sfc.template_tokens.push(SerializableToken::from(VToken::new(
      kind,
      Span::new(start as u32, end as u32),
      value,
    )));
  }

  fn read_tag(&self, start: usize) -> Option<TagInfo> {
    let mut pos = start + 1;
    let is_end = self.byte(pos) == b'/';
    if is_end {
      pos += 1;
    }
    let name_start = pos;
    while pos < self.source_text.len() {
      let b = self.byte(pos);
      if !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        break;
      }
      pos += 1;
    }
    if pos == name_start {
      return None;
    }
    let name_end = pos;
    let tag_end = self.find_tag_end(pos)?;
    let mut close_probe = tag_end;
    while close_probe > name_end && self.byte(close_probe - 1).is_ascii_whitespace() {
      close_probe -= 1;
    }
    Some(TagInfo {
      name_start,
      name_end,
      tag_end,
      is_end,
      is_self_closing: close_probe > name_end && self.byte(close_probe - 1) == b'/',
    })
  }

  fn find_tag_end(&self, mut pos: usize) -> Option<usize> {
    while pos < self.source_text.len() {
      match self.byte(pos) {
        b'\'' | b'"' => {
          let quote = self.byte(pos);
          pos = self.find_byte(pos + 1, self.source_text.len(), quote)? + 1;
        }
        b'>' => return Some(pos),
        _ => pos += 1,
      }
    }
    None
  }

  fn find_ascii_case_insensitive(&self, start: usize, needle: &str) -> Option<usize> {
    let bytes = self.source_text.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || start + needle.len() > bytes.len() {
      return None;
    }
    (start..=bytes.len() - needle.len()).find(|&i| {
      bytes[i..i + needle.len()].iter().zip(needle).all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
  }

  fn lower_source(&self, start: usize, end: usize) -> String {
    self.source_text[start..end].to_ascii_lowercase()
  }

  fn alloc_str(&self, value: &str) -> &'b str {
    self.js_allocator.alloc_str(value)
  }

  fn skip_ws(&self, mut pos: usize, end: usize) -> usize {
    while pos < end && self.byte(pos).is_ascii_whitespace() {
      pos += 1;
    }
    pos
  }

  fn trim_end_ws(&self, start: usize, mut end: usize) -> usize {
    while end > start && self.byte(end - 1).is_ascii_whitespace() {
      end -= 1;
    }
    end
  }

  fn skip_quoted(&self, mut pos: usize, end: usize) -> usize {
    let quote = self.byte(pos);
    pos += 1;
    while pos < end {
      match self.byte(pos) {
        b'\\' => pos = (pos + 2).min(end),
        byte if byte == quote => return pos + 1,
        _ => pos += 1,
      }
    }
    pos
  }

  fn skip_template_literal(&self, mut pos: usize, end: usize) -> usize {
    pos += 1;
    while pos < end {
      match self.byte(pos) {
        b'\\' => pos = (pos + 2).min(end),
        b'`' => return pos + 1,
        _ => pos += 1,
      }
    }
    pos
  }

  fn find_byte(&self, start: usize, end: usize, byte: u8) -> Option<usize> {
    self.source_text.as_bytes()[start..end].iter().position(|b| *b == byte).map(|i| start + i)
  }

  fn find_from(&self, start: usize, needle: &str) -> Option<usize> {
    self.source_text[start..].find(needle).map(|i| start + i)
  }

  fn read_character_reference(&self, start: usize, end: usize) -> Option<(usize, &'b str)> {
    let name_start = start + 1;
    let mut name_end = name_start;
    while name_end < end && self.byte(name_end).is_ascii_alphanumeric() {
      name_end += 1;
    }
    if name_end >= end || self.byte(name_end) != b';' {
      return None;
    }

    let value = match &self.source_text[name_start..name_end] {
      "amp" => "&",
      "lt" => "<",
      "gt" => ">",
      "quot" => "\"",
      "apos" => "'",
      _ => return None,
    };

    Some((name_end + 1, self.alloc_str(value)))
  }

  fn starts_with(&self, pos: usize, needle: &str) -> bool {
    self.source_text[pos..].starts_with(needle)
  }

  fn byte(&self, pos: usize) -> u8 {
    self.source_text.as_bytes()[pos]
  }
}

fn attr_value<'s>(source: &'s str, name: &str) -> Option<&'s str> {
  let start = source.find(name)? + name.len();
  let rest = source[start..].trim_start();
  let rest = rest.strip_prefix('=')?.trim_start();
  let quote = rest.as_bytes().first().copied()?;
  if !matches!(quote, b'\'' | b'"') {
    return None;
  }
  let value_start = 1;
  let value_end = rest[value_start..].find(quote as char)? + value_start;
  Some(&rest[value_start..value_end])
}

fn attr_value_kind(attr_name: &str) -> AttrValueKind {
  if attr_name == "v-for" {
    return AttrValueKind::VFor;
  }

  if attr_name == "v-on" || attr_name.starts_with("v-on:") {
    return AttrValueKind::Handler;
  }

  if attr_name.as_bytes().first() == Some(&b'@') && attr_name.len() > 1 {
    return AttrValueKind::Handler;
  }

  if (attr_name.as_bytes().first() == Some(&b'#') && attr_name.len() > 1)
    || (attr_name.starts_with("v-slot:") && !attr_name.ends_with(':'))
  {
    return AttrValueKind::SlotParams;
  }

  if (attr_name.as_bytes().first() == Some(&b':') && attr_name.len() > 1)
    || attr_name == "v-bind"
    || matches!(attr_name, "v-if" | "v-else-if" | "v-model")
    || attr_name.starts_with("v-bind:")
  {
    return AttrValueKind::Expression;
  }

  AttrValueKind::Literal
}

fn is_raw_text_element(name: &str) -> bool {
  matches!(
    name,
    "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript" | "plaintext"
  )
}

fn is_rcdata_element(name: &str) -> bool {
  matches!(name, "textarea" | "title")
}

fn is_foreign_content(element_stack: &[String]) -> bool {
  element_stack.iter().rev().any(|name| matches!(name.as_str(), "svg" | "math"))
}

fn pop_element(element_stack: &mut Vec<String>, name: &str) {
  if let Some(index) = element_stack.iter().rposition(|current| current == name) {
    element_stack.truncate(index);
  }
}
