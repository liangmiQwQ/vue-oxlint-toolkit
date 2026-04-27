//! Minimal HTML/template tokenizer + parser producing V* AST nodes.
//!
//! This is intentionally a *small* HTML5 subset: enough to handle Vue
//! templates that use balanced element nesting, attribute syntax (including
//! Vue's directive shorthands `:`, `@`, `#`), and `{{ ... }}` mustache
//! interpolations. It is not a conformant HTML5 tokenizer — that level of
//! fidelity is not needed for Phase 1 and would dwarf the rest of this crate.
//!
//! Anything ambiguous (raw text content of `<script>`/`<style>`, comments,
//! CDATA, HTML entity decoding) is left as a TODO and noted in-line.

use oxc_allocator::{Allocator, Box as ArenaBox, Vec as ArenaVec};

use crate::ast::{
  Span, VAttribute, VAttributeKey, VAttributeValue, VDirectiveKey, VElement, VElementChild,
  VEndTag, VExpressionContainer, VIdentifier, VLiteral, VNamespace, VStartTag, VText,
};

/// Parse the contents of a `<template>` block (the inner HTML, not including
/// the surrounding tag itself) into a vector of element children.
///
/// `body_offset` is the byte offset of `body` within the original SFC
/// source — used to translate local spans into source-absolute spans.
pub fn parse_template_body<'a>(
  alloc: &'a Allocator,
  body: &'a str,
  body_offset: u32,
) -> ArenaVec<'a, VElementChild<'a>> {
  let mut p = TemplateParser { alloc, src: body, base: body_offset, pos: 0 };
  p.parse_children(None)
}

/// Parse a single element from a top-level SFC block (i.e. `<template>`,
/// `<script>`, `<style>`, or a custom block). The caller has already
/// identified the tag span / attributes substring; this function only
/// builds the `VElement` shell. The element's children are populated from
/// `inner_body` (which may be empty for blocks we treat as opaque).
pub fn build_block_element<'a>(
  alloc: &'a Allocator,
  source: &'a str,
  tag: &'a str,
  range: Span,
  start_tag_range: Span,
  end_tag_range: Option<Span>,
  raw_attributes: &'a str,
  raw_attributes_offset: u32,
  self_closing: bool,
  children: ArenaVec<'a, VElementChild<'a>>,
) -> ArenaBox<'a, VElement<'a>> {
  let attrs = parse_attributes(alloc, source, raw_attributes, raw_attributes_offset);
  let start_tag = ArenaBox::new_in(
    VStartTag { r#type: "VStartTag", range: start_tag_range, self_closing, attributes: attrs },
    alloc,
  );
  let end_tag =
    end_tag_range.map(|r| ArenaBox::new_in(VEndTag { r#type: "VEndTag", range: r }, alloc));
  ArenaBox::new_in(
    VElement {
      r#type: "VElement",
      range,
      name: tag,
      raw_name: tag,
      namespace: VNamespace::Html,
      start_tag,
      end_tag,
      children,
    },
    alloc,
  )
}

struct TemplateParser<'a> {
  alloc: &'a Allocator,
  src: &'a str,
  base: u32,
  pos: usize,
}

impl<'a> TemplateParser<'a> {
  fn bytes(&self) -> &'a [u8] {
    self.src.as_bytes()
  }

  fn span(&self, lo: usize, hi: usize) -> Span {
    Span::new(self.base + lo as u32, self.base + hi as u32)
  }

  fn parse_children(&mut self, parent_close: Option<&str>) -> ArenaVec<'a, VElementChild<'a>> {
    let mut out: ArenaVec<'a, VElementChild<'a>> = ArenaVec::new_in(self.alloc);
    let bytes = self.bytes();
    let len = bytes.len();
    let mut text_start = self.pos;
    while self.pos < len {
      let b = bytes[self.pos];
      // Mustache start `{{`
      if b == b'{' && self.pos + 1 < len && bytes[self.pos + 1] == b'{' {
        self.flush_text(text_start, self.pos, &mut out);
        if let Some(node) = self.parse_mustache() {
          out.push(VElementChild::ExpressionContainer(node));
        }
        text_start = self.pos;
        continue;
      }
      if b == b'<' && self.pos + 1 < len {
        let next = bytes[self.pos + 1];
        // End tag — defer to caller.
        if next == b'/' {
          if let Some(name) = parent_close
            && self.matches_end_tag(name)
          {
            self.flush_text(text_start, self.pos, &mut out);
            return out;
          }
          // Stray end tag — consume it as text.
          self.pos += 1;
          continue;
        }
        if next == b'!' {
          // Comment / doctype / cdata — skip until `>`.
          self.flush_text(text_start, self.pos, &mut out);
          while self.pos < len && bytes[self.pos] != b'>' {
            self.pos += 1;
          }
          if self.pos < len {
            self.pos += 1;
          }
          text_start = self.pos;
          continue;
        }
        if next.is_ascii_alphabetic() {
          self.flush_text(text_start, self.pos, &mut out);
          if let Some(el) = self.parse_element() {
            out.push(VElementChild::Element(el));
            text_start = self.pos;
            continue;
          }
        }
      }
      self.pos += 1;
    }
    self.flush_text(text_start, self.pos, &mut out);
    out
  }

  fn flush_text(&self, lo: usize, hi: usize, out: &mut ArenaVec<'a, VElementChild<'a>>) {
    if hi > lo {
      let text = ArenaBox::new_in(
        VText { r#type: "VText", range: self.span(lo, hi), value: &self.src[lo..hi] },
        self.alloc,
      );
      out.push(VElementChild::Text(text));
    }
  }

  fn matches_end_tag(&self, name: &str) -> bool {
    let bytes = self.bytes();
    let need = b"</";
    if !bytes[self.pos..].starts_with(need) {
      return false;
    }
    let after = self.pos + 2;
    let nb = name.as_bytes();
    if after + nb.len() > bytes.len() {
      return false;
    }
    if !bytes[after..after + nb.len()].eq_ignore_ascii_case(nb) {
      return false;
    }
    matches!(bytes.get(after + nb.len()), Some(b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n') | None)
  }

  fn parse_mustache(&mut self) -> Option<ArenaBox<'a, VExpressionContainer<'a>>> {
    let bytes = self.bytes();
    let lo = self.pos;
    self.pos += 2; // `{{`
    let expr_lo = self.pos;
    while self.pos + 1 < bytes.len() && !(bytes[self.pos] == b'}' && bytes[self.pos + 1] == b'}') {
      self.pos += 1;
    }
    if self.pos + 1 >= bytes.len() {
      // Unterminated mustache — treat as text by reverting.
      self.pos = lo + 1;
      return None;
    }
    let expr_hi = self.pos;
    self.pos += 2; // `}}`
    let raw = self.src[expr_lo..expr_hi].trim();
    Some(ArenaBox::new_in(
      VExpressionContainer {
        r#type: "VExpressionContainer",
        range: self.span(lo, self.pos),
        raw_expression: raw,
        expression_range: self.span(expr_lo, expr_hi),
        raw: false,
      },
      self.alloc,
    ))
  }

  fn parse_element(&mut self) -> Option<ArenaBox<'a, VElement<'a>>> {
    let bytes = self.bytes();
    let lo = self.pos;
    self.pos += 1; // `<`
    let name_lo = self.pos;
    while self.pos < bytes.len() && is_tag_name_part(bytes[self.pos]) {
      self.pos += 1;
    }
    if self.pos == name_lo {
      self.pos = lo + 1;
      return None;
    }
    let name = &self.src[name_lo..self.pos];
    let attrs_lo = self.pos;
    let (start_tag_end, self_closing) = match find_tag_end(bytes, self.pos) {
      Some(v) => v,
      None => {
        self.pos = lo + 1;
        return None;
      }
    };
    let attrs_hi = start_tag_end - if self_closing { 2 } else { 1 };
    let raw_attrs = &self.src[attrs_lo..attrs_hi];
    let attrs = parse_attributes(self.alloc, self.src, raw_attrs, self.base + attrs_lo as u32);
    self.pos = start_tag_end;

    let start_tag = ArenaBox::new_in(
      VStartTag {
        r#type: "VStartTag",
        range: self.span(lo, start_tag_end),
        self_closing,
        attributes: attrs,
      },
      self.alloc,
    );

    if self_closing || is_void_html_element(name) {
      return Some(ArenaBox::new_in(
        VElement {
          r#type: "VElement",
          range: self.span(lo, start_tag_end),
          name,
          raw_name: name,
          namespace: VNamespace::Html,
          start_tag,
          end_tag: None,
          children: ArenaVec::new_in(self.alloc),
        },
        self.alloc,
      ));
    }

    let children = self.parse_children(Some(name));
    let mut end_tag = None;
    let element_end;
    if self.matches_end_tag(name) {
      let end_lo = self.pos;
      self.pos += 2 + name.len();
      while self.pos < bytes.len() && bytes[self.pos] != b'>' {
        self.pos += 1;
      }
      if self.pos < bytes.len() {
        self.pos += 1;
      }
      element_end = self.pos;
      end_tag = Some(ArenaBox::new_in(
        VEndTag { r#type: "VEndTag", range: self.span(end_lo, element_end) },
        self.alloc,
      ));
    } else {
      // No matching end tag found before EOF — close at current pos.
      element_end = self.pos;
    }

    Some(ArenaBox::new_in(
      VElement {
        r#type: "VElement",
        range: self.span(lo, element_end),
        name,
        raw_name: name,
        namespace: VNamespace::Html,
        start_tag,
        end_tag,
        children,
      },
      self.alloc,
    ))
  }
}

fn find_tag_end(bytes: &[u8], mut pos: usize) -> Option<(usize, bool)> {
  while pos < bytes.len() {
    match bytes[pos] {
      b'>' => return Some((pos + 1, false)),
      b'/' if pos + 1 < bytes.len() && bytes[pos + 1] == b'>' => return Some((pos + 2, true)),
      b'"' | b'\'' => {
        let q = bytes[pos];
        pos += 1;
        while pos < bytes.len() && bytes[pos] != q {
          pos += 1;
        }
        if pos < bytes.len() {
          pos += 1;
        }
      }
      _ => pos += 1,
    }
  }
  None
}

const fn is_tag_name_part(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':' || b == b'.'
}

fn is_void_html_element(name: &str) -> bool {
  matches!(
    name.to_ascii_lowercase().as_str(),
    "area"
      | "base"
      | "br"
      | "col"
      | "embed"
      | "hr"
      | "img"
      | "input"
      | "link"
      | "meta"
      | "param"
      | "source"
      | "track"
      | "wbr"
  )
}

/// Parse the attribute list found between the tag name and the closing `>`
/// of a start tag. `raw` is the substring; `raw_offset` is its absolute
/// offset in the original SFC source.
pub fn parse_attributes<'a>(
  alloc: &'a Allocator,
  _full_source: &'a str,
  raw: &'a str,
  raw_offset: u32,
) -> ArenaVec<'a, VAttribute<'a>> {
  let mut out: ArenaVec<'a, VAttribute<'a>> = ArenaVec::new_in(alloc);
  let bytes = raw.as_bytes();
  let len = bytes.len();
  let mut i = 0usize;
  while i < len {
    while i < len && bytes[i].is_ascii_whitespace() {
      i += 1;
    }
    if i >= len {
      break;
    }
    let key_lo = i;
    while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
      i += 1;
    }
    if i == key_lo {
      i += 1;
      continue;
    }
    let key_hi = i;
    let key_text = &raw[key_lo..key_hi];

    // Skip whitespace before `=`
    let mut probe = i;
    while probe < len && bytes[probe].is_ascii_whitespace() {
      probe += 1;
    }
    let mut value: Option<(usize, usize, &str)> = None;
    let mut attr_hi = key_hi;
    if probe < len && bytes[probe] == b'=' {
      probe += 1;
      while probe < len && bytes[probe].is_ascii_whitespace() {
        probe += 1;
      }
      if probe < len {
        let q = bytes[probe];
        if q == b'"' || q == b'\'' {
          let v_lo = probe + 1;
          let mut v_hi = v_lo;
          while v_hi < len && bytes[v_hi] != q {
            v_hi += 1;
          }
          let txt = &raw[v_lo..v_hi];
          value = Some((v_lo, v_hi, txt));
          attr_hi = v_hi + 1;
          i = v_hi + if v_hi < len { 1 } else { 0 };
        } else {
          let v_lo = probe;
          let mut v_hi = probe;
          while v_hi < len && !bytes[v_hi].is_ascii_whitespace() {
            v_hi += 1;
          }
          let txt = &raw[v_lo..v_hi];
          value = Some((v_lo, v_hi, txt));
          attr_hi = v_hi;
          i = v_hi;
        }
      } else {
        i = probe;
      }
    } else {
      i = probe;
    }

    let key_span = Span::new(raw_offset + key_lo as u32, raw_offset + key_hi as u32);
    let attr_span = Span::new(raw_offset + key_lo as u32, raw_offset + attr_hi as u32);

    let (key_node, is_directive) = classify_key(alloc, key_text, key_span);
    let value_node = value.map(|(v_lo, v_hi, txt)| {
      let span = Span::new(raw_offset + v_lo as u32, raw_offset + v_hi as u32);
      if is_directive {
        ArenaBox::new_in(
          VAttributeValue::Expression(VExpressionContainer {
            r#type: "VExpressionContainer",
            range: span,
            raw_expression: txt,
            expression_range: span,
            raw: false,
          }),
          alloc,
        )
      } else {
        ArenaBox::new_in(
          VAttributeValue::Literal(VLiteral { r#type: "VLiteral", range: span, value: txt }),
          alloc,
        )
      }
    });

    out.push(VAttribute {
      r#type: "VAttribute",
      range: attr_span,
      directive: is_directive,
      key: key_node,
      value: value_node,
    });
  }
  out
}

fn classify_key<'a>(
  alloc: &'a Allocator,
  raw: &'a str,
  span: Span,
) -> (ArenaBox<'a, VAttributeKey<'a>>, bool) {
  let bytes = raw.as_bytes();
  if bytes.is_empty() {
    return (
      ArenaBox::new_in(
        VAttributeKey::Identifier(VIdentifier {
          r#type: "VIdentifier",
          range: span,
          name: raw,
          raw_name: raw,
        }),
        alloc,
      ),
      false,
    );
  }
  let (name, arg_offset, _shorthand) = match bytes[0] {
    b':' => ("bind", 1, true),
    b'@' => ("on", 1, true),
    b'#' => ("slot", 1, true),
    _ if raw.starts_with("v-") => {
      // `v-name[:arg][.mod]*`
      let after = &raw[2..];
      let name_end = after.find(|c: char| c == ':' || c == '.').map_or(after.len(), |idx| idx);
      let name = &after[..name_end];
      let consumed = 2 + name_end;
      return parse_directive_key(alloc, raw, name, consumed, span);
    }
    _ => {
      return (
        ArenaBox::new_in(
          VAttributeKey::Identifier(VIdentifier {
            r#type: "VIdentifier",
            range: span,
            name: raw,
            raw_name: raw,
          }),
          alloc,
        ),
        false,
      );
    }
  };
  parse_directive_key(alloc, raw, name, arg_offset, span)
}

fn parse_directive_key<'a>(
  alloc: &'a Allocator,
  raw: &'a str,
  name: &'a str,
  consumed: usize,
  span: Span,
) -> (ArenaBox<'a, VAttributeKey<'a>>, bool) {
  // After `consumed` chars, optionally `:arg`, then `.mod` parts.
  let rest = &raw[consumed..];
  let (argument, after_arg_idx) = if let Some(rest_after_colon) = rest.strip_prefix(':') {
    let dot = rest_after_colon.find('.').unwrap_or(rest_after_colon.len());
    let arg = &rest_after_colon[..dot];
    (Some(arg), consumed + 1 + dot)
  } else if !rest.is_empty() && rest.as_bytes()[0] != b'.' && consumed < raw.len() {
    // Shorthand argument follows immediately (e.g. `:foo`, `@click`, `#default`).
    let dot = rest.find('.').unwrap_or(rest.len());
    let arg = &rest[..dot];
    if arg.is_empty() { (None, consumed) } else { (Some(arg), consumed + dot) }
  } else {
    (None, consumed)
  };

  let mut modifiers: ArenaVec<'a, &'a str> = ArenaVec::new_in(alloc);
  let mut tail = &raw[after_arg_idx..];
  while let Some(t) = tail.strip_prefix('.') {
    let next = t.find('.').unwrap_or(t.len());
    modifiers.push(&t[..next]);
    tail = &t[next..];
  }

  (
    ArenaBox::new_in(
      VAttributeKey::Directive(VDirectiveKey {
        r#type: "VDirectiveKey",
        range: span,
        name,
        argument,
        modifiers,
        raw,
      }),
      alloc,
    ),
    true,
  )
}
