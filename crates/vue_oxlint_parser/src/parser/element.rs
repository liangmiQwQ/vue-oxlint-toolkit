//! Element and children parsing for Vue SFC tokenizer.

use crate::ast::{VCData, VComment, VElement, VEndTag, VNode, VStartTag, VText};
use crate::parser::Parser;
use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span;

/// Raw text elements: their content is never parsed for child elements/interpolations
const RAW_TEXT_TAGS: &[&str] =
  &["script", "style", "textarea", "iframe", "xmp", "noembed", "noframes", "noscript"];

fn is_raw_text(tag: &str) -> bool {
  RAW_TEXT_TAGS.contains(&tag)
}

/// Void elements that have no end tag
const VOID_TAGS: &[&str] = &[
  "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
  "track", "wbr",
];

fn is_void_tag(tag: &str) -> bool {
  VOID_TAGS.contains(&tag)
}

impl<'a> Parser<'a> {
  /// Parse a list of children nodes, stopping at `</end_tag>` or EOF.
  /// If `end_tag` is None, we parse until EOF (top-level SFC children).
  pub fn parse_children(&mut self, end_tag: Option<&str>) -> Vec<VNode<'a>> {
    let mut children = Vec::new();

    loop {
      if self.is_eof() {
        break;
      }

      // Check for end tag
      if self.matches("</") {
        if let Some(tag) = end_tag {
          // Peek ahead to see if this closing tag matches
          if self.peek_end_tag_name() == tag {
            break;
          }
        }
        // Unmatched end tag or top-level end tag — consume and warn
        if end_tag.is_none() {
          self.skip_end_tag();
        } else {
          break;
        }
      } else if self.matches("<!--") {
        children.push(VNode::Comment(self.parse_comment()));
      } else if self.matches("<![CDATA[") {
        children.push(VNode::CData(self.parse_cdata()));
      } else if self.matches("{{") {
        if let Some(interp) = self.parse_interpolation() {
          children.push(VNode::Interpolation(interp));
        }
      } else if self.current_byte() == Some(b'<') {
        match self.parse_element() {
          Some(elem) => children.push(VNode::Element(elem)),
          None => break,
        }
      } else {
        // Text node
        let text = self.parse_text();
        if !text.raw.is_empty() {
          children.push(VNode::Text(text));
        }
      }
    }

    children
  }

  /// Peek ahead to get the name of the next closing tag `</name>`
  fn peek_end_tag_name(&self) -> &str {
    let bytes = self.source_text.as_bytes();
    let mut i = self.pos + 2; // skip '</'
    // skip whitespace
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
      i += 1;
    }
    let start = i;
    while i < bytes.len()
      && bytes[i] != b'>'
      && bytes[i] != b'/'
      && bytes[i] != b' '
      && bytes[i] != b'\t'
      && bytes[i] != b'\n'
      && bytes[i] != b'\r'
    {
      i += 1;
    }
    &self.source_text[start..i]
  }

  /// Consume and discard an end tag `</foo>`
  fn skip_end_tag(&mut self) {
    // skip '</...'
    while !self.is_eof() && self.current_byte() != Some(b'>') {
      self.pos += 1;
    }
    if self.current_byte() == Some(b'>') {
      self.pos += 1;
    }
  }

  /// Consume an end tag `</foo>`, returning its span
  pub fn consume_end_tag(&mut self) -> Option<VEndTag> {
    if !self.matches("</") {
      return None;
    }
    let start = self.pos_u32();
    // skip '</'
    self.advance(2);
    // skip name
    while !self.is_eof() && self.current_byte() != Some(b'>') && self.current_byte() != Some(b'/') {
      self.pos += 1;
    }
    // consume '>'
    if self.current_byte() == Some(b'>') {
      self.pos += 1;
    }
    let end = self.pos_u32();
    Some(VEndTag { span: Span::new(start, end) })
  }

  /// Parse an HTML comment `<!-- value -->`
  pub fn parse_comment(&mut self) -> VComment {
    let start = self.pos_u32();
    debug_assert!(self.matches("<!--"));
    self.advance(4);

    let value_start = self.pos;
    loop {
      if self.is_eof() {
        let value = self.source_text[value_start..self.pos].to_string();
        self.push_error(OxcDiagnostic::error("Unexpected EOF inside comment"));
        return VComment { value, span: Span::new(start, self.pos_u32()) };
      }
      if self.matches("-->") {
        let value = self.source_text[value_start..self.pos].to_string();
        self.advance(3);
        return VComment { value, span: Span::new(start, self.pos_u32()) };
      }
      self.pos += 1;
    }
  }

  /// Parse CDATA `<![CDATA[...]]>`
  pub fn parse_cdata(&mut self) -> VCData {
    let start = self.pos_u32();
    debug_assert!(self.matches("<![CDATA["));
    self.advance(9);

    let value_start = self.pos;
    loop {
      if self.is_eof() {
        let value = self.source_text[value_start..self.pos].to_string();
        self.push_error(OxcDiagnostic::error("Unexpected EOF inside CDATA"));
        return VCData { value, span: Span::new(start, self.pos_u32()) };
      }
      if self.matches("]]>") {
        let value = self.source_text[value_start..self.pos].to_string();
        self.advance(3);
        return VCData { value, span: Span::new(start, self.pos_u32()) };
      }
      self.pos += 1;
    }
  }

  /// Parse a mustache interpolation `{{ expr }}`
  pub fn parse_interpolation(&mut self) -> Option<crate::ast::VInterpolation<'a>> {
    use crate::ast::VInterpolation;

    let start = self.pos_u32();
    debug_assert!(self.matches("{{"));
    self.advance(2);

    let expr_start = self.pos;
    loop {
      if self.is_eof() {
        self.push_error(OxcDiagnostic::error("Unexpected EOF inside interpolation"));
        return None;
      }
      if self.matches("}}") {
        break;
      }
      self.pos += 1;
    }
    let expr_end = self.pos;
    self.advance(2); // skip '}}'
    let span = Span::new(start, self.pos_u32());

    if expr_start == expr_end {
      return Some(VInterpolation { expression: None, span });
    }

    let expr_span = Span::new(expr_start as u32, expr_end as u32);
    let expression = self.parse_expression_in_interpolation(expr_span);
    Some(VInterpolation { expression, span })
  }

  /// Parse a text node (raw text up to `<`, `{{`, or EOF)
  pub fn parse_text(&mut self) -> VText {
    let start = self.pos_u32();
    while !self.is_eof() && !self.matches("<") && !self.matches("{{") {
      self.pos += 1;
    }
    let end = self.pos_u32();
    let raw = self.source_text[start as usize..end as usize].to_string();
    VText {
      value: raw.clone(), // entity decoding deferred
      raw,
      span: Span::new(start, end),
    }
  }

  /// Parse raw-text element body content (no child parsing)
  fn parse_raw_text_content(&mut self, tag_name: &str) -> (String, Span) {
    let start = self.pos_u32();
    let close = format!("</{tag_name}");
    let close_lower = close.to_lowercase();

    loop {
      if self.is_eof() {
        break;
      }
      // Case-insensitive match for end tag
      let remaining = &self.source_text[self.pos..];
      if remaining.len() >= close_lower.len()
        && remaining[..close_lower.len()].to_lowercase() == close_lower
      {
        break;
      }
      self.pos += 1;
    }

    let end = self.pos_u32();
    let raw = self.source_text[start as usize..end as usize].to_string();
    (raw, Span::new(start, end))
  }

  /// Parse an element starting at `<`. Returns None on unrecoverable error.
  pub fn parse_element(&mut self) -> Option<VElement<'a>> {
    let elem_start = self.pos_u32();

    // Parse start tag
    let (start_tag, is_self_closing) = self.parse_start_tag()?;
    let tag_name = self.slice(start_tag.name_span.start, start_tag.name_span.end).to_string();

    if is_self_closing || is_void_tag(&tag_name) {
      let span = Span::new(elem_start, self.pos_u32());
      return Some(VElement {
        start_tag,
        end_tag: None,
        children: Vec::new(),
        span,
        program: None,
      });
    }

    // Parse children
    let (children, program) = if is_raw_text(&tag_name) {
      let (raw, content_span) = self.parse_raw_text_content(&tag_name);

      // For script elements, parse the JS
      let program = if tag_name == "script" {
        self.parse_script_content(&start_tag, content_span)
      } else {
        None
      };

      let text_children = if raw.is_empty() {
        Vec::new()
      } else {
        vec![VNode::Text(VText { value: raw.clone(), raw, span: content_span })]
      };

      (text_children, program)
    } else {
      (self.parse_children(Some(&tag_name)), None)
    };

    // Consume end tag
    let end_tag = self.consume_end_tag();

    let span = Span::new(elem_start, self.pos_u32());

    Some(VElement { start_tag, end_tag, children, span, program })
  }

  /// Parse `<name attrs...>` or `<name attrs... />`.
  /// Returns `(VStartTag, self_closing)`.
  pub fn parse_start_tag(&mut self) -> Option<(VStartTag<'a>, bool)> {
    if self.current_byte() != Some(b'<') {
      return None;
    }
    let start = self.pos_u32();
    self.advance(1); // skip '<'

    // Read tag name
    let name_start = self.pos_u32();
    while let Some(b) = self.current_byte() {
      if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'>' || b == b'/' {
        break;
      }
      self.pos += 1;
    }
    let name_end = self.pos_u32();

    if name_start == name_end {
      // Empty tag name: not an element, skip
      return None;
    }

    // Skip whitespace
    self.skip_whitespace();

    // Parse attributes
    let mut attributes = Vec::new();
    loop {
      self.skip_whitespace();
      if self.is_eof() {
        self.push_error(OxcDiagnostic::error("Unexpected EOF in start tag"));
        return None;
      }
      match self.current_byte()? {
        b'>' => {
          self.advance(1);
          let end = self.pos_u32();
          let name_span = Span::new(name_start, name_end);
          let span = Span::new(start, end);
          return Some((VStartTag { name_span, attributes, self_closing: false, span }, false));
        }
        b'/' if self.peek_byte(1) == Some(b'>') => {
          self.advance(2);
          let end = self.pos_u32();
          let name_span = Span::new(name_start, name_end);
          let span = Span::new(start, end);
          return Some((VStartTag { name_span, attributes, self_closing: true, span }, true));
        }
        _ => {
          if let Some(attr) = self.parse_attribute() {
            attributes.push(attr);
          } else {
            // Skip one char to avoid infinite loop
            self.pos += 1;
          }
        }
      }
    }
  }
}
