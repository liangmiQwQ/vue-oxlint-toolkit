//! SFC top-level parser.
//!
//! Drives the lexer over the entire source, identifying top-level blocks
//! (`<template>`, `<script>`, `<style>`, custom tags) and the whitespace
//! text between them. Each block records the attribute tokens emitted by
//! the lexer so they can be replayed (without re-scanning) when the
//! caller needs `lang=` detection or full V* attribute construction.

use oxc_allocator::{Allocator, Box as ArenaBox, Vec as ArenaVec};
use oxc_span::SourceType;
use oxc_str::Str;

use crate::ast::{
  Span, VAttribute, VDocumentFragment, VElement, VElementChild, VEndTag, VNamespace, VRootChild,
  VStartTag, VText,
};
use crate::lexer::Lexer;
use crate::token::{LexMode, Token, TokenKind};

use super::attr::{self, AttrTok};
use super::template;

/// Layout entry for a top-level SFC block.
pub struct LayoutBlock<'a> {
  pub tag: &'a str,
  pub range: Span,
  pub start_tag_range: Span,
  pub end_tag_range: Option<Span>,
  pub content_range: Span,
  pub attrs: Vec<AttrTok<'a>>,
  pub self_closing: bool,
}

/// SFC scan result: the ordered list of top-level blocks and the text
/// segments interleaved between them.
pub struct ParsedLayout<'a> {
  pub blocks: Vec<LayoutBlock<'a>>,
  pub text_segments: Vec<(Span, &'a str)>,
}

/// Drive the lexer over `source` and capture the SFC layout.
pub fn parse_sfc<'a>(_alloc: &'a Allocator, source: &'a str) -> ParsedLayout<'a> {
  let mut lexer = Lexer::new(source);
  let mut blocks: Vec<LayoutBlock<'a>> = Vec::new();
  let mut texts: Vec<(Span, &'a str)> = Vec::new();
  let mut text_lo: u32 = 0;
  let total_len = source.len() as u32;

  loop {
    let tok = lexer.next();
    match tok.kind {
      TokenKind::Eof => break,
      TokenKind::TagOpen { name, .. } => {
        // Flush any pending text before the tag.
        if tok.span.start > text_lo {
          texts.push((
            Span::new(text_lo, tok.span.start),
            &source[text_lo as usize..tok.span.start as usize],
          ));
        }
        let block = read_block(&mut lexer, source, name, tok.span);
        text_lo = block.range.end;
        // Skip past the block body and end tag — we don't tokenize it
        // here. Block contents are interpreted later by either the
        // template parser or the JS parser.
        lexer.seek(block.range.end);
        lexer.set_mode(LexMode::Data);
        blocks.push(block);
      }
      // Stray tokens at the top level — fold into the text run.
      _ => {
        // Continue accumulating; text segments are emitted at TagOpen / EOF.
      }
    }
  }

  if text_lo < total_len {
    texts.push((Span::new(text_lo, total_len), &source[text_lo as usize..total_len as usize]));
  }

  ParsedLayout { blocks, text_segments: texts }
}

fn read_block<'a>(
  lexer: &mut Lexer<'a>,
  source: &'a str,
  name: &'a str,
  open_span: Span,
) -> LayoutBlock<'a> {
  // The lexer just emitted TagOpen and is now positioned right after the
  // tag name. Switch to InTag and read attributes until `>` or `/>`.
  lexer.set_mode(LexMode::InTag);
  let (attrs, end_tok) = read_attrs(lexer);
  let self_closing = matches!(end_tok.kind, TokenKind::TagSelfClose);
  let start_tag_range = Span::new(open_span.start, end_tok.span.end);
  lexer.set_mode(LexMode::Data);

  if self_closing {
    return LayoutBlock {
      tag: name,
      range: start_tag_range,
      start_tag_range,
      end_tag_range: None,
      content_range: Span::new(start_tag_range.end, start_tag_range.end),
      attrs,
      self_closing: true,
    };
  }

  // Skip the body until the matching `</name>` end tag. We don't lex the
  // body here — block contents are interpreted later (template body via
  // the template parser; script/style as raw text).
  let body_lo = start_tag_range.end;
  let (content_end, end_tag_range) = scan_block_body(source, name, body_lo);

  LayoutBlock {
    tag: name,
    range: Span::new(open_span.start, end_tag_range.map_or(content_end, |s| s.end)),
    start_tag_range,
    end_tag_range,
    content_range: Span::new(body_lo, content_end),
    attrs,
    self_closing: false,
  }
}

fn read_attrs<'a>(lexer: &mut Lexer<'a>) -> (Vec<AttrTok<'a>>, Token<'a>) {
  super::read_start_tag_attrs(lexer)
}

/// Scan the source for the matching `</name>` end tag, ignoring nested
/// occurrences of identical tags (Vue SFCs forbid these at the top level
/// but we tolerate it leniently). Returns `(content_end, end_tag_span)`.
fn scan_block_body(source: &str, name: &str, body_lo: u32) -> (u32, Option<Span>) {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let close_marker = format!("</{name}");
  let close_b = close_marker.as_bytes();
  let open_marker = format!("<{name}");
  let open_b = open_marker.as_bytes();
  let mut depth: i32 = 1;
  let mut k = body_lo as usize;
  while k < len {
    if bytes[k] == b'<' {
      if k + close_b.len() <= len
        && bytes[k..k + close_b.len()].eq_ignore_ascii_case(close_b)
        && (k + close_b.len() == len
          || matches!(bytes[k + close_b.len()], b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n'))
      {
        depth -= 1;
        if depth == 0 {
          let content_end = k as u32;
          let mut m = k + close_b.len();
          while m < len && bytes[m] != b'>' {
            m += 1;
          }
          let end_close = if m < len { m + 1 } else { m };
          return (content_end, Some(Span::new(content_end, end_close as u32)));
        }
      } else if k + open_b.len() <= len
        && bytes[k..k + open_b.len()].eq_ignore_ascii_case(open_b)
        && (k + open_b.len() == len
          || matches!(bytes[k + open_b.len()], b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n'))
      {
        depth += 1;
      }
    }
    k += 1;
  }
  // Unclosed top-level block — extend to EOF without an end tag.
  (len as u32, None)
}

/// Build the document tree from a parsed layout. `<template>` children are
/// fully parsed into V* nodes; other blocks expose their body as a single
/// `VText` so byte coverage matches the source.
pub fn build_document<'a>(
  alloc: &'a Allocator,
  source: &'a str,
  layout: &ParsedLayout<'a>,
  template_source_type: SourceType,
) -> ArenaBox<'a, VDocumentFragment<'a>> {
  let mut root: ArenaVec<'a, VRootChild<'a>> = ArenaVec::new_in(alloc);

  let total_len = source.len() as u32;
  let mut block_iter = layout.blocks.iter().peekable();
  let mut text_iter = layout.text_segments.iter().peekable();
  let mut next_offset = 0u32;

  while next_offset < total_len {
    if let Some(&(span, txt)) = text_iter.peek().copied()
      && span.start == next_offset
    {
      let node =
        ArenaBox::new_in(VText { r#type: "VText", range: span, value: Str::from(txt) }, alloc);
      root.push(VRootChild::Text(node));
      next_offset = span.end;
      text_iter.next();
      continue;
    }
    if let Some(block) = block_iter.peek()
      && block.range.start == next_offset
    {
      let block = block_iter.next().unwrap();
      let element = build_block_element(alloc, source, block, template_source_type);
      next_offset = block.range.end;
      root.push(VRootChild::Element(element));
      continue;
    }
    break;
  }

  ArenaBox::new_in(VDocumentFragment::new(Span::new(0, total_len), root), alloc)
}

fn build_block_element<'a>(
  alloc: &'a Allocator,
  source: &'a str,
  block: &LayoutBlock<'a>,
  template_source_type: SourceType,
) -> ArenaBox<'a, VElement<'a>> {
  let attrs: ArenaVec<'a, VAttribute<'a>> = attr::build_vattributes(
    alloc,
    &block.attrs,
    /* in_v_pre */ false,
    SourceType::default().with_module(true).with_typescript(true),
  );

  let start_tag = ArenaBox::new_in(
    VStartTag {
      r#type: "VStartTag",
      range: block.start_tag_range,
      self_closing: block.self_closing,
      attributes: attrs,
    },
    alloc,
  );
  let end_tag =
    block.end_tag_range.map(|r| ArenaBox::new_in(VEndTag { r#type: "VEndTag", range: r }, alloc));

  let children: ArenaVec<'a, VElementChild<'a>> =
    if block.tag.eq_ignore_ascii_case("template") && !block.self_closing {
      let body = &source[block.content_range.start as usize..block.content_range.end as usize];
      template::parse_template_body(alloc, body, block.content_range.start, template_source_type)
    } else {
      let mut v: ArenaVec<'a, VElementChild<'a>> = ArenaVec::new_in(alloc);
      if !block.self_closing && block.content_range.end > block.content_range.start {
        let txt = &source[block.content_range.start as usize..block.content_range.end as usize];
        let n = ArenaBox::new_in(
          VText { r#type: "VText", range: block.content_range, value: Str::from(txt) },
          alloc,
        );
        v.push(VElementChild::Text(n));
      }
      v
    };

  ArenaBox::new_in(
    VElement {
      r#type: "VElement",
      range: block.range,
      name: Str::from(block.tag),
      raw_name: Str::from(block.tag),
      namespace: VNamespace::Html,
      start_tag,
      end_tag,
      children,
    },
    alloc,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn splits_basic_sfc() {
    let alloc = Allocator::default();
    let src = "<template>\n  <div>{{ x }}</div>\n</template>\n<script>let x = 1</script>\n";
    let layout = parse_sfc(&alloc, src);
    assert_eq!(layout.blocks.len(), 2);
    assert_eq!(layout.blocks[0].tag, "template");
    assert_eq!(layout.blocks[1].tag, "script");
  }

  #[test]
  fn handles_self_closing_block() {
    let alloc = Allocator::default();
    let src = "<template src=\"./x.html\" />\n";
    let layout = parse_sfc(&alloc, src);
    assert_eq!(layout.blocks.len(), 1);
    assert!(layout.blocks[0].self_closing);
  }
}
