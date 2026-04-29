//! Template body parser.
//!
//! Recursive-descent parser that consumes the lexer's token stream to
//! build a `VElement` tree. Drives the lexer mode for raw-text /
//! RCDATA elements (`<script>`, `<style>`, `<textarea>`) and tracks the
//! ancestor chain so HTML5 implicit-close rules (`<p>`, `<dt>`/`<dd>`,
//! `<li>`, ruby) can fire on stray ancestor end tags.

use oxc_allocator::{Allocator, Box as ArenaBox, Vec as ArenaVec};
use oxc_span::SourceType;
use oxc_str::Str;

use crate::ast::{
  Span, VElement, VElementChild, VEndTag, VExprKind, VExpressionContainer, VNamespace, VStartTag,
  VText,
};
use crate::lexer::Lexer;
use crate::lexer::{LexMode, Token, TokenKind};

use super::attr::{self, AttrTok, AttrValue};

/// Parse the inner body of a `<template>` block into V* children.
pub fn parse_template_body<'a>(
  alloc: &'a Allocator,
  body: &'a str,
  body_offset: u32,
  source_type: SourceType,
) -> ArenaVec<'a, VElementChild<'a>> {
  let mut lexer = Lexer::new(body);
  let mut p = Parser {
    alloc,
    body,
    body_offset,
    lexer: &mut lexer,
    source_type,
    open_stack: Vec::new(),
    in_v_pre: false,
    pending: None,
  };
  p.parse_children(&[])
}

struct Parser<'a, 'l> {
  alloc: &'a Allocator,
  body: &'a str,
  /// Offset in the original SFC source that `body` starts at.
  body_offset: u32,
  lexer: &'l mut Lexer<'a>,
  source_type: SourceType,
  open_stack: Vec<&'a str>,
  in_v_pre: bool,
  /// One-token lookahead buffer.
  pending: Option<Token<'a>>,
}

impl<'a> Parser<'a, '_> {
  const fn shift(&self, span: Span) -> Span {
    Span::new(self.body_offset + span.start, self.body_offset + span.end)
  }

  fn next(&mut self) -> Token<'a> {
    self.pending.take().unwrap_or_else(|| self.lexer.next())
  }

  fn peek(&mut self) -> Token<'a> {
    if self.pending.is_none() {
      self.pending = Some(self.lexer.next());
    }
    self.pending.unwrap()
  }

  /// Merge adjacent `VText` children that come from contiguous source
  /// regions. This unifies `text + v-pre mustache + text` into one `VText`,
  /// matching upstream behavior.
  fn coalesce_text(
    &self,
    list: ArenaVec<'a, VElementChild<'a>>,
  ) -> ArenaVec<'a, VElementChild<'a>> {
    let mut out: ArenaVec<'a, VElementChild<'a>> = ArenaVec::new_in(self.alloc);
    for item in list {
      if let VElementChild::Text(ref new_text) = item
        && let Some(VElementChild::Text(prev_text)) = out.last_mut()
        && prev_text.range.end == new_text.range.start
      {
        let start = prev_text.range.start;
        let end = new_text.range.end;
        let body_start = (start - self.body_offset) as usize;
        let body_end = (end - self.body_offset) as usize;
        let merged = &self.body[body_start..body_end];
        prev_text.range = Span::new(start, end);
        prev_text.value = Str::from(merged);
        continue;
      }
      out.push(item);
    }
    out
  }

  fn parse_children(&mut self, ancestors: &[&str]) -> ArenaVec<'a, VElementChild<'a>> {
    let parent_close = ancestors.first().copied();
    let mut out: ArenaVec<'a, VElementChild<'a>> = ArenaVec::new_in(self.alloc);
    loop {
      let tok = self.peek();
      match tok.kind {
        TokenKind::Eof => break,
        TokenKind::Text { text } => {
          self.next();
          if !text.is_empty() {
            out.push(self.make_text(tok.span, text));
          }
        }
        TokenKind::Mustache { expr, expr_span } => {
          self.next();
          if self.in_v_pre {
            // Treat the whole mustache span as raw text.
            let raw = self.body_text(tok.span);
            out.push(self.make_text(tok.span, raw));
          } else {
            out.push(VElementChild::ExpressionContainer(
              self.make_expr_container(tok.span, expr, expr_span),
            ));
          }
        }
        TokenKind::EndTag { name, .. } => {
          if let Some(p) = parent_close
            && name.eq_ignore_ascii_case(p)
          {
            // Don't consume — caller will pick this up.
            return self.coalesce_text(out);
          }
          if ancestors.iter().skip(1).any(|n| name.eq_ignore_ascii_case(n)) {
            // Stray end tag of an outer ancestor — let the outer frame
            // handle it.
            return self.coalesce_text(out);
          }
          // Stray end tag for nothing — silently swallow.
          self.next();
        }
        TokenKind::TagOpen { name, .. } => {
          if let Some(parent) = ancestors.first()
            && auto_closes(parent, name)
          {
            return self.coalesce_text(out);
          }
          let element = self.parse_element();
          if let Some(el) = element {
            out.push(VElementChild::Element(el));
          }
        }
        TokenKind::Comment
        | TokenKind::Cdata
        | TokenKind::Bang
        | TokenKind::ProcessingInstruction => {
          self.next();
        }
        _ => {
          // Defensive: should not appear at the top of Data mode.
          self.next();
        }
      }
    }
    self.coalesce_text(out)
  }

  fn parse_element(&mut self) -> Option<ArenaBox<'a, VElement<'a>>> {
    let open_tok = self.next();
    let TokenKind::TagOpen { name, .. } = open_tok.kind else {
      return None;
    };
    let lo = open_tok.span.start;
    self.lexer.set_mode(LexMode::InTag);
    let (attrs, end_tok) = read_attrs(self.lexer);
    self.lexer.set_mode(LexMode::Data);
    let self_closing = matches!(end_tok.kind, TokenKind::TagSelfClose);
    let start_tag_end = end_tok.span.end;
    let start_tag_range = Span::new(lo, start_tag_end);

    let has_v_pre = attr::has_v_pre(&attrs);
    let attributes = attr::build_vattributes(
      self.alloc,
      &shift_attrs(&attrs, self.body_offset, self.alloc),
      has_v_pre || self.in_v_pre,
      self.source_type,
    );

    let start_tag = ArenaBox::new_in(
      VStartTag::new(self.shift(start_tag_range), self_closing, attributes),
      self.alloc,
    );

    if self_closing || is_void_html_element(name) {
      return Some(ArenaBox::new_in(
        VElement::new(
          self.shift(start_tag_range),
          Str::from(name),
          Str::from(name),
          VNamespace::Html,
          start_tag,
          None,
          ArenaVec::new_in(self.alloc),
        ),
        self.alloc,
      ));
    }

    self.open_stack.push(name);
    let prev_v_pre = self.in_v_pre;
    self.in_v_pre = prev_v_pre || has_v_pre;

    let chain: Vec<&str> = self.open_stack.iter().rev().copied().collect();
    let children = if is_raw_text_element(name) {
      self.parse_raw_text_children(name)
    } else if is_rcdata_element(name) {
      self.parse_rcdata_children(name)
    } else {
      self.parse_children(&chain)
    };

    self.open_stack.pop();
    self.in_v_pre = prev_v_pre;

    // Consume the matching end tag, if any.
    let mut end_tag = None;
    let element_end;
    let next = self.peek();
    if let TokenKind::EndTag { name: n, .. } = next.kind
      && n.eq_ignore_ascii_case(name)
    {
      let t = self.next();
      element_end = t.span.end;
      end_tag = Some(ArenaBox::new_in(VEndTag::new(self.shift(t.span)), self.alloc));
    } else {
      // No matching end tag. Element ends just before the next pending
      // token (if any), else at the lexer's current position.
      element_end = self.pending.map_or_else(|| self.lexer.pos(), |t| t.span.start);
    }

    Some(ArenaBox::new_in(
      VElement::new(
        self.shift(Span::new(lo, element_end)),
        Str::from(name),
        Str::from(name),
        VNamespace::Html,
        start_tag,
        end_tag,
        children,
      ),
      self.alloc,
    ))
  }

  fn parse_raw_text_children(&mut self, tag_name: &'a str) -> ArenaVec<'a, VElementChild<'a>> {
    self.lexer.set_mode(LexMode::RawText { name: tag_name });
    let mut out: ArenaVec<'a, VElementChild<'a>> = ArenaVec::new_in(self.alloc);
    loop {
      let t = self.peek();
      match t.kind {
        TokenKind::Text { text } => {
          self.next();
          if !text.is_empty() {
            out.push(self.make_text(t.span, text));
          }
        }
        _ => break,
      }
    }
    self.lexer.set_mode(LexMode::Data);
    out
  }

  fn parse_rcdata_children(&mut self, tag_name: &'a str) -> ArenaVec<'a, VElementChild<'a>> {
    self.lexer.set_mode(LexMode::RcData { name: tag_name });
    let mut out: ArenaVec<'a, VElementChild<'a>> = ArenaVec::new_in(self.alloc);
    loop {
      let t = self.peek();
      match t.kind {
        TokenKind::Text { text } => {
          self.next();
          if !text.is_empty() {
            out.push(self.make_text(t.span, text));
          }
        }
        TokenKind::Mustache { expr, expr_span } => {
          self.next();
          if self.in_v_pre {
            let raw = self.body_text(t.span);
            out.push(self.make_text(t.span, raw));
          } else {
            out.push(VElementChild::ExpressionContainer(
              self.make_expr_container(t.span, expr, expr_span),
            ));
          }
        }
        _ => break,
      }
    }
    self.lexer.set_mode(LexMode::Data);
    self.coalesce_text(out)
  }

  fn make_text(&self, span: Span, text: &'a str) -> VElementChild<'a> {
    VElementChild::Text(ArenaBox::new_in(VText::new(self.shift(span), Str::from(text)), self.alloc))
  }

  fn make_expr_container(
    &self,
    outer: Span,
    expr: &'a str,
    expr_span: Span,
  ) -> ArenaBox<'a, VExpressionContainer<'a>> {
    ArenaBox::new_in(
      VExpressionContainer::new(
        self.shift(outer),
        Str::from(expr),
        self.shift(expr_span),
        false,
        false,
        VExprKind::Default,
        self.source_type,
      ),
      self.alloc,
    )
  }

  fn body_text(&self, span: Span) -> &'a str {
    &self.body[span.start as usize..span.end as usize]
  }
}

fn read_attrs<'a>(lexer: &mut Lexer<'a>) -> (Vec<AttrTok<'a>>, Token<'a>) {
  super::read_start_tag_attrs(lexer)
}

fn shift_attrs<'a>(attrs: &[AttrTok<'a>], base: u32, _alloc: &'a Allocator) -> Vec<AttrTok<'a>> {
  attrs
    .iter()
    .map(|a| AttrTok {
      key_span: Span::new(base + a.key_span.start, base + a.key_span.end),
      key: a.key,
      value: a.value.map(|v| AttrValue {
        outer_span: Span::new(base + v.outer_span.start, base + v.outer_span.end),
        inner_span: Span::new(base + v.inner_span.start, base + v.inner_span.end),
        text: v.text,
        quoted: v.quoted,
      }),
      attr_end: base + a.attr_end,
    })
    .collect()
}

fn auto_closes(parent: &str, child: &str) -> bool {
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

#[allow(clippy::missing_const_for_fn)]
fn is_raw_text_element(name: &str) -> bool {
  matches!(name, "style" | "script")
}

#[allow(clippy::missing_const_for_fn)]
fn is_rcdata_element(name: &str) -> bool {
  name.eq_ignore_ascii_case("textarea")
}
