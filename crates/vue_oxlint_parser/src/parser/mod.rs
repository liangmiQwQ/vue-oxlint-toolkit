mod irregular_whitespaces;
mod module_record;
mod oxc_parse;

use crate::ast::token::SerializableToken;
use crate::lexer::{VToken, VTokenKind};
use crate::parser::irregular_whitespaces::collect_irregular_whitespaces;
use crate::{VueParser, VueParserReturn};
use oxc_span::{SourceType, Span};

#[derive(Debug, Clone, Copy)]
struct TagInfo {
  name_start: usize,
  name_end: usize,
  tag_end: usize,
  is_end: bool,
  is_self_closing: bool,
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

    while pos < len {
      if self.starts_with(pos, "<!--") {
        pos = self.find_from(pos + 4, "-->").map_or(len, |end| end + 3);
        continue;
      }

      if self.byte(pos) == b'<'
        && let Some(tag) = self.read_tag(pos)
      {
        self.emit_tag(&tag);
        pos = tag.tag_end + 1;

        if !tag.is_end && !tag.is_self_closing {
          let name = self.lower_source(tag.name_start, tag.name_end);
          if matches!(name.as_str(), "script" | "style") {
            pos = self.parse_raw_element_body(pos, &name, &mut seen_script, &mut seen_setup);
          }
        }
        continue;
      }

      let next = self.find_from(pos + 1, "<").unwrap_or(len);
      self.emit_data(pos, next);
      pos = next;
    }
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
      self.emit_oxc_tokens(start + 1, end - 1);
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
    if should_parse_attr_value(attr_name) {
      if token_start < value_start {
        let quote = &self.source_text[token_start..value_start];
        self.push_template_token(VTokenKind::Punctuator, token_start, value_start, Some(quote));
      }
      self.emit_oxc_tokens(value_start, value_end);
      if value_end < token_end {
        let quote = &self.source_text[value_end..token_end];
        self.push_template_token(VTokenKind::Punctuator, value_end, token_end, Some(quote));
      }
    } else {
      self.push_template_token(
        VTokenKind::HTMLLiteral,
        token_start,
        token_end,
        Some(&self.source_text[value_start..value_end]),
      );
    }
  }

  fn emit_data(&mut self, start: usize, end: usize) {
    let mut pos = start;
    while pos < end {
      if self.starts_with(pos, "{{")
        && let Some(close) = self.find_from(pos + 2, "}}")
      {
        self.push_template_token(VTokenKind::VExpressionStart, pos, pos + 2, Some("{{"));
        self.emit_oxc_tokens(pos + 2, close);
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

  fn emit_oxc_tokens(&mut self, start: usize, end: usize) {
    self.emit_js_like_tokens(start, end);
  }

  fn emit_js_like_tokens(&mut self, start: usize, end: usize) {
    let mut pos = start;
    while pos < end {
      let b = self.byte(pos);
      if b.is_ascii_whitespace() {
        pos += 1;
        continue;
      }

      if self.starts_with(pos, "/*") {
        pos = self.find_from(pos + 2, "*/").map_or(end, |comment_end| comment_end + 2);
        continue;
      }
      if self.starts_with(pos, "//") {
        pos = self.find_from(pos + 2, "\n").unwrap_or(end);
        continue;
      }

      if matches!(b, b'\'' | b'"') {
        let token_start = pos;
        pos += 1;
        while pos < end {
          match self.byte(pos) {
            b'\\' => pos = (pos + 2).min(end),
            quote if quote == b => {
              pos += 1;
              break;
            }
            _ => pos += 1,
          }
        }
        self.push_template_token(
          VTokenKind::String,
          token_start,
          pos,
          Some(&self.source_text[token_start..pos]),
        );
        continue;
      }

      if b.is_ascii_digit() {
        let token_start = pos;
        pos += 1;
        while pos < end {
          let b = self.byte(pos);
          if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.') {
            pos += 1;
          } else {
            break;
          }
        }
        self.push_template_token(
          VTokenKind::Numeric,
          token_start,
          pos,
          Some(&self.source_text[token_start..pos]),
        );
        continue;
      }

      if is_ident_start(b) {
        let token_start = pos;
        pos += 1;
        while pos < end && is_ident_continue(self.byte(pos)) {
          pos += 1;
        }
        let value = &self.source_text[token_start..pos];
        let kind = if value == "in" { VTokenKind::Keyword } else { VTokenKind::Identifier };
        self.push_template_token(kind, token_start, pos, Some(value));
        continue;
      }

      let token_start = pos;
      pos += 1;
      self.push_template_token(
        VTokenKind::Punctuator,
        token_start,
        pos,
        Some(&self.source_text[token_start..pos]),
      );
    }
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

  fn find_byte(&self, start: usize, end: usize, byte: u8) -> Option<usize> {
    self.source_text.as_bytes()[start..end].iter().position(|b| *b == byte).map(|i| start + i)
  }

  fn find_from(&self, start: usize, needle: &str) -> Option<usize> {
    self.source_text[start..].find(needle).map(|i| start + i)
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

fn should_parse_attr_value(attr_name: &str) -> bool {
  (matches!(attr_name.as_bytes().first(), Some(b':' | b'@' | b'#')) && attr_name.len() > 1)
    || attr_name == "v-bind"
    || matches!(attr_name, "v-if" | "v-else-if" | "v-for" | "v-model")
    || attr_name.starts_with("v-bind:")
    || (attr_name.starts_with("v-slot:") && !attr_name.ends_with(':'))
}

const fn is_ident_start(b: u8) -> bool {
  b.is_ascii_alphabetic() || matches!(b, b'_' | b'$')
}

const fn is_ident_continue(b: u8) -> bool {
  is_ident_start(b) || b.is_ascii_digit() || b == b'-'
}
