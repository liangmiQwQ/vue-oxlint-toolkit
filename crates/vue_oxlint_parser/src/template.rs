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
use oxc_str::Str;

use crate::ast::{
  Span, VAttribute, VAttributeKey, VAttributeValue, VDirectiveKey, VDirectiveKeyArgument, VElement,
  VElementChild, VEndTag, VExpressionContainer, VIdentifier, VLiteral, VNamespace, VStartTag,
  VText,
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
  let mut p =
    TemplateParser { alloc, src: body, base: body_offset, pos: 0, open_stack: Vec::new() };
  p.parse_children(&[])
}

/// Parse a single element from a top-level SFC block.
///
/// Handles `<template>`, `<script>`, `<style>`, or a custom block. The caller
/// has already identified the tag span / attributes substring; this function
/// only builds the `VElement` shell. The element's children are populated
/// from `inner_body` (which may be empty for blocks we treat as opaque).
#[allow(clippy::too_many_arguments)]
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
      name: Str::from(tag),
      raw_name: Str::from(tag),
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
  /// Names of currently-open elements, outermost first. Used to detect
  /// implicit-close on stray ancestor end tags and on auto-closing siblings.
  open_stack: Vec<&'a str>,
}

impl<'a> TemplateParser<'a> {
  const fn bytes(&self) -> &'a [u8] {
    self.src.as_bytes()
  }

  const fn span(&self, lo: usize, hi: usize) -> Span {
    Span::new(self.base + lo as u32, self.base + hi as u32)
  }

  fn parse_children(&mut self, ancestors: &[&str]) -> ArenaVec<'a, VElementChild<'a>> {
    let parent_close = ancestors.first().copied();
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
        // End tag.
        if next == b'/' {
          if let Some(name) = parent_close
            && self.matches_end_tag(name)
          {
            self.flush_text(text_start, self.pos, &mut out);
            return out;
          }
          // End tag for an outer ancestor — close the current frame so
          // the ancestor can match the tag. (HTML5 implicit close.)
          if ancestors.iter().skip(1).any(|n| self.matches_end_tag(n)) {
            self.flush_text(text_start, self.pos, &mut out);
            return out;
          }
          // Stray end tag — silently consume up to and including `>`.
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
          // Auto-close the current element when its content model
          // forbids the upcoming start tag (HTML5: `<p>` is closed by
          // any flow-level sibling, `<dt>`/`<dd>` close each other).
          if let Some(parent) = ancestors.first()
            && let Some(name) = peek_tag_name(self.bytes(), self.pos + 1)
            && auto_closes(parent, name)
          {
            self.flush_text(text_start, self.pos, &mut out);
            return out;
          }
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
        VText { r#type: "VText", range: self.span(lo, hi), value: Str::from(&self.src[lo..hi]) },
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
    let raw = &self.src[expr_lo..expr_hi];
    Some(ArenaBox::new_in(
      VExpressionContainer {
        r#type: "VExpressionContainer",
        range: self.span(lo, self.pos),
        raw_expression: Str::from(raw),
        expression_range: self.span(expr_lo, expr_hi),
        raw: false,
        synthetic_identifier: false,
        kind: crate::ast::VExprKind::Default,
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
    let (start_tag_end, self_closing) = find_tag_end(bytes, self.pos);
    let trim = if self_closing { 2 } else { 1 };
    let attrs_hi = start_tag_end.saturating_sub(trim).max(attrs_lo);
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
          name: Str::from(name),
          raw_name: Str::from(name),
          namespace: VNamespace::Html,
          start_tag,
          end_tag: None,
          children: ArenaVec::new_in(self.alloc),
        },
        self.alloc,
      ));
    }

    self.open_stack.push(name);
    // `parse_children` wants the ancestor chain with the immediate parent
    // first; our stack is outermost-first, so reverse on the way in.
    let chain: Vec<&str> = self.open_stack.iter().rev().copied().collect();
    let children = self.parse_children(&chain);
    self.open_stack.pop();
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
        name: Str::from(name),
        raw_name: Str::from(name),
        namespace: VNamespace::Html,
        start_tag,
        end_tag,
        children,
      },
      self.alloc,
    ))
  }
}

/// Inspect the argument-and-tail slice of a directive key (everything after
/// the directive name and a possible separating `:`) and classify it into:
/// `(arg_offset_in_raw, arg_text, end_of_arg_in_raw, is_dynamic)`.
///
/// `arg_offset_in_raw` is the position of the first byte of the argument
/// span within the parent raw string (the start of `[` for dynamic args, or
/// of the static identifier text otherwise). `end_of_arg_in_raw` points one
/// past the last byte of the argument span (just past `]` or just before the
/// first `.modifier`).
fn is_plausible_arg_name(s: &str) -> bool {
  let bytes = s.as_bytes();
  if bytes.is_empty() {
    return false;
  }
  // Must start with a letter or underscore; later bytes may include `-`.
  if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
    return false;
  }
  bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

fn classify_arg(after: &str, base_in_raw: usize) -> (usize, &str, usize, bool) {
  after.strip_prefix('[').map_or_else(
    || {
      let dot = after.find('.').unwrap_or(after.len());
      (base_in_raw, &after[..dot], base_in_raw + dot, false)
    },
    |rest| {
      // Find matching `]`. When unclosed, fall back to a static-identifier
      // argument with the literal `[…` text — matches upstream parse-error
      // recovery, which does not attempt to interpret a malformed bracket
      // group as a dynamic argument.
      rest.find(']').map_or_else(
        || {
          let dot = after.find('.').unwrap_or(after.len());
          (base_in_raw, &after[..dot], base_in_raw + dot, false)
        },
        |end_inner| {
          let inner = &rest[..end_inner];
          let consumed = end_inner + 2;
          (base_in_raw, inner, base_in_raw + consumed, true)
        },
      )
    },
  )
}

fn find_tag_end(bytes: &[u8], mut pos: usize) -> (usize, bool) {
  while pos < bytes.len() {
    match bytes[pos] {
      b'>' => return (pos + 1, false),
      b'/' if pos + 1 < bytes.len() && bytes[pos + 1] == b'>' => return (pos + 2, true),
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
  // Tolerate EOF in start tag: return current position with no `>` consumed.
  (pos, false)
}

const fn is_tag_name_part(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':' || b == b'.'
}

/// Peek the tag name starting at `pos` (which should point one past `<`).
/// Returns `None` if no tag-name characters follow.
fn peek_tag_name(bytes: &[u8], pos: usize) -> Option<&str> {
  let mut end = pos;
  while end < bytes.len() && is_tag_name_part(bytes[end]) {
    end += 1;
  }
  if end == pos {
    return None;
  }
  std::str::from_utf8(&bytes[pos..end]).ok()
}

/// HTML5 content-model-driven auto-close: when `parent` is open and `child`
/// is encountered as a sibling start tag, should `parent` be implicitly
/// closed before `child` opens?
fn auto_closes(parent: &str, child: &str) -> bool {
  // Only apply HTML5 implicit-close to plain lowercase element names.
  // Custom components (`<Address>`, `<my-comp>`) follow Vue's component
  // resolution and are not subject to native content-model rules.
  if !child.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
    return false;
  }
  if child.contains('-') {
    return false;
  }
  let p = parent.to_ascii_lowercase();
  let c = child.to_ascii_lowercase();
  match p.as_str() {
    "p" => matches!(
      c.as_str(),
      "address"
        | "article"
        | "aside"
        | "blockquote"
        | "details"
        | "div"
        | "dl"
        | "fieldset"
        | "figcaption"
        | "figure"
        | "footer"
        | "form"
        | "h1"
        | "h2"
        | "h3"
        | "h4"
        | "h5"
        | "h6"
        | "header"
        | "hgroup"
        | "hr"
        | "main"
        | "menu"
        | "nav"
        | "ol"
        | "p"
        | "pre"
        | "section"
        | "table"
        | "ul"
    ),
    "dt" | "dd" => matches!(c.as_str(), "dt" | "dd"),
    "li" => c == "li",
    "rb" | "rt" | "rp" | "rtc" => matches!(c.as_str(), "rb" | "rt" | "rp" | "rtc"),
    _ => false,
  }
}

fn is_void_html_element(name: &str) -> bool {
  matches!(
    name,
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
    // (span_lo, span_hi, inner_lo, inner_hi, inner_text). span_lo..span_hi
    // covers the value as it appears in source (including any quote chars);
    // inner_lo..inner_hi covers just the unquoted inner expression text.
    let mut value: Option<(usize, usize, usize, usize, &str)> = None;
    let mut attr_hi = key_hi;
    if probe < len && bytes[probe] == b'=' {
      probe += 1;
      attr_hi = probe;
      while probe < len && bytes[probe].is_ascii_whitespace() {
        probe += 1;
      }
      if probe < len {
        let q = bytes[probe];
        if q == b'"' || q == b'\'' {
          let inner_lo = probe + 1;
          let mut inner_hi = inner_lo;
          while inner_hi < len && bytes[inner_hi] != q {
            inner_hi += 1;
          }
          let txt = &raw[inner_lo..inner_hi];
          let closed = inner_hi < len;
          let span_hi = if closed { inner_hi + 1 } else { inner_hi };
          value = Some((probe, span_hi, inner_lo, inner_hi, txt));
          attr_hi = span_hi;
          i = span_hi;
        } else {
          let v_lo = probe;
          let mut v_hi = probe;
          while v_hi < len && !bytes[v_hi].is_ascii_whitespace() && bytes[v_hi] != b'>' {
            v_hi += 1;
          }
          let txt = &raw[v_lo..v_hi];
          value = Some((v_lo, v_hi, v_lo, v_hi, txt));
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

    // Determine the directive kind so the value container can be parsed
    // appropriately (statement-list for v-on, binding pattern for v-slot,
    // alias-and-iter for v-for).
    let value_kind: crate::ast::VExprKind = if let VAttributeKey::Directive(dk) = &*key_node {
      let n = dk.name.name.as_str();
      if n == "@" || n.eq_ignore_ascii_case("v-on") {
        crate::ast::VExprKind::VOn
      } else if n == "#" || n.eq_ignore_ascii_case("v-slot") || n.eq_ignore_ascii_case("slot-scope")
      {
        crate::ast::VExprKind::VSlot
      } else if n.eq_ignore_ascii_case("v-for") {
        crate::ast::VExprKind::VFor
      } else {
        crate::ast::VExprKind::Default
      }
    } else {
      crate::ast::VExprKind::Default
    };

    // `v-bind` same-name shorthand: a `:foo` / `v-bind:foo` directive with
    // a static argument and no value synthesises a value expression
    // container whose expression is an `Identifier` named like the argument.
    let synth_value: Option<(Span, &'a str)> = {
      let mut s = None;
      if value.is_none()
        && is_directive
        && let VAttributeKey::Directive(dk) = &*key_node
      {
        let name_str = dk.name.name;
        let is_bind = name_str.as_ref() == ":" || name_str.as_ref().eq_ignore_ascii_case("v-bind");
        if is_bind && let Some(VDirectiveKeyArgument::Identifier(id)) = &dk.argument {
          let n = id.name.as_str();
          // Skip synthesis when the argument text isn't a plausible
          // identifier — e.g. the parse-error fallback for an unclosed
          // dynamic-argument bracket leaves text like `["a` here.
          if !n.is_empty() && is_plausible_arg_name(n) {
            s = Some((id.range, n));
          }
        }
      }
      s
    };

    let value_node = value.map(|(v_lo, v_hi, inner_lo, inner_hi, txt)| {
      let span = Span::new(raw_offset + v_lo as u32, raw_offset + v_hi as u32);
      let inner_span = Span::new(raw_offset + inner_lo as u32, raw_offset + inner_hi as u32);
      if is_directive {
        ArenaBox::new_in(
          VAttributeValue::Expression(VExpressionContainer {
            r#type: "VExpressionContainer",
            range: span,
            raw_expression: Str::from(txt),
            expression_range: inner_span,
            raw: false,
            synthetic_identifier: false,
            kind: value_kind,
          }),
          alloc,
        )
      } else {
        ArenaBox::new_in(
          VAttributeValue::Literal(VLiteral {
            r#type: "VLiteral",
            range: span,
            value: Str::from(txt),
          }),
          alloc,
        )
      }
    });

    let value_node = value_node.or_else(|| {
      synth_value.map(|(arg_span, name)| {
        ArenaBox::new_in(
          VAttributeValue::Expression(VExpressionContainer {
            r#type: "VExpressionContainer",
            range: arg_span,
            raw_expression: Str::from(name),
            expression_range: arg_span,
            raw: false,
            synthetic_identifier: true,
            kind: crate::ast::VExprKind::Default,
          }),
          alloc,
        )
      })
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
          name: Str::from(raw),
          raw_name: Str::from(raw),
        }),
        alloc,
      ),
      false,
    );
  }
  // Determine the directive `name` slice (and its byte length) from the raw
  // source. Shorthands keep their literal prefix character as the name, to
  // match upstream `vue-eslint-parser` behavior.
  let name_len = match bytes[0] {
    b':' | b'@' | b'#' | b'.' => 1,
    _ if raw.starts_with("v-") => {
      let after = &raw[2..];
      let after_end = after.find([':', '.']).unwrap_or(after.len());
      2 + after_end
    }
    _ => {
      return (
        ArenaBox::new_in(
          VAttributeKey::Identifier(VIdentifier {
            r#type: "VIdentifier",
            range: span,
            name: Str::from(raw),
            raw_name: Str::from(raw),
          }),
          alloc,
        ),
        false,
      );
    }
  };
  parse_directive_key(alloc, raw, name_len, span)
}

#[allow(clippy::option_if_let_else)]
fn parse_directive_key<'a>(
  alloc: &'a Allocator,
  raw: &'a str,
  name_len: usize,
  span: Span,
) -> (ArenaBox<'a, VAttributeKey<'a>>, bool) {
  let name_text = &raw[..name_len];
  let name_ident = VIdentifier {
    r#type: "VIdentifier",
    range: Span::new(span.start, span.start + name_len as u32),
    name: Str::from(name_text),
    raw_name: Str::from(name_text),
  };

  // After the name, optionally `:arg` (only for `v-foo` form) or the
  // shorthand argument that follows the prefix immediately, then `.mod`s.
  let rest = &raw[name_len..];
  let bytes0 = raw.as_bytes()[0];
  let is_shorthand = matches!(bytes0, b':' | b'@' | b'#' | b'.');
  let is_prop_shorthand = bytes0 == b'.';

  let (arg_offset, arg_text, after_arg_idx, dynamic) =
    if !is_shorthand && let Some(after_colon) = rest.strip_prefix(':') {
      classify_arg(after_colon, name_len + 1)
    } else if is_shorthand && !rest.is_empty() && rest.as_bytes()[0] != b'.' {
      classify_arg(rest, name_len)
    } else {
      (name_len, "", name_len, false)
    };

  let argument = if arg_text.is_empty() && !dynamic {
    None
  } else if dynamic {
    // Dynamic argument: `[expr]`. The argument node is a
    // VExpressionContainer covering the brackets; its inner expression
    // covers just the text inside.
    let outer = Span::new(span.start + arg_offset as u32, span.start + after_arg_idx as u32);
    let inner = Span::new(outer.start + 1, outer.end - 1);
    Some(VDirectiveKeyArgument::Expression(VExpressionContainer {
      r#type: "VExpressionContainer",
      range: outer,
      raw_expression: Str::from(arg_text),
      expression_range: inner,
      raw: false,
      synthetic_identifier: false,
      kind: crate::ast::VExprKind::Default,
    }))
  } else {
    Some(VDirectiveKeyArgument::Identifier(VIdentifier {
      r#type: "VIdentifier",
      range: Span::new(span.start + arg_offset as u32, span.start + after_arg_idx as u32),
      name: Str::from(arg_text),
      raw_name: Str::from(arg_text),
    }))
  };

  let mut modifiers: ArenaVec<'a, VIdentifier<'a>> = ArenaVec::new_in(alloc);
  let mut cursor = after_arg_idx;
  while cursor < raw.len() && raw.as_bytes()[cursor] == b'.' {
    let mod_lo = cursor + 1;
    let rest = &raw[mod_lo..];
    let dot = rest.find('.').unwrap_or(rest.len());
    let mod_hi = mod_lo + dot;
    let text = &raw[mod_lo..mod_hi];
    modifiers.push(VIdentifier {
      r#type: "VIdentifier",
      range: Span::new(span.start + mod_lo as u32, span.start + mod_hi as u32),
      name: Str::from(text),
      raw_name: Str::from(text),
    });
    cursor = mod_hi;
  }

  // The `.` shorthand for `v-bind:foo.prop` always has the implicit `prop`
  // modifier. When the user wrote it as `.foo` (no explicit modifiers),
  // synthesise an empty-named modifier at the end of the key span — this
  // matches upstream's marker for "prop shorthand without explicit prop".
  if is_prop_shorthand && modifiers.is_empty() {
    let end = span.end;
    modifiers.push(VIdentifier {
      r#type: "VIdentifier",
      range: Span::new(end, end),
      name: Str::from(""),
      raw_name: Str::from(""),
    });
  }

  (
    ArenaBox::new_in(
      VAttributeKey::Directive(VDirectiveKey {
        r#type: "VDirectiveKey",
        range: span,
        name: name_ident,
        argument,
        modifiers,
        raw: Str::from(raw),
      }),
      alloc,
    ),
    true,
  )
}
