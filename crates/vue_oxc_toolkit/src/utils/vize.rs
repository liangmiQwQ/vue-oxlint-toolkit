//! Extension traits over [`vize_armature`] node types.
//!
//! Vize reports source locations with a few quirks that don't translate
//! directly to JSX spans. Each helper in this module documents the quirk it
//! adapts so callers don't have to re-discover it.

use oxc_span::Span;
use vize_armature::{
  AttributeNode, DirectiveNode, ElementNode, ElementType, ExpressionNode, SourceLocation,
};

use super::text::roffset;

/// Convert a vize source-bearing node into an [`oxc_span::Span`].
pub trait VizeSpan {
  fn span(&self) -> Span;
}

impl VizeSpan for SourceLocation {
  fn span(&self) -> Span {
    Span::new(self.start.offset, self.end.offset)
  }
}

impl VizeSpan for ExpressionNode<'_> {
  fn span(&self) -> Span {
    self.loc().span()
  }
}

/// Helpers over [`ElementNode`].
pub trait ElementExt {
  /// Span covering just the tag name (the chars right after `<`).
  fn name_span(&self) -> Span;

  /// True when the element should be rendered as a JSX component reference.
  ///
  /// vize classifies user components as [`ElementType::Component`], but
  /// Vue's built-in dynamic `<component is="...">` is reported as a plain
  /// [`ElementType::Element`] — we promote it manually so it survives
  /// downstream identifier-resolution passes.
  fn is_component_like(&self) -> bool;

  /// True end of this element in source — vize's `loc` only covers the
  /// opening tag, so for non-void/non-self-closing elements we recover the
  /// closing-tag end by scanning the source.
  fn true_end_offset(&self, source: &str, is_void: bool) -> u32;
}

impl ElementExt for ElementNode<'_> {
  fn name_span(&self) -> Span {
    Span::sized(self.loc.start.offset + 1, self.tag.len() as u32)
  }

  fn is_component_like(&self) -> bool {
    matches!(self.tag_type, ElementType::Component) || self.tag.as_str() == "component"
  }

  fn true_end_offset(&self, source: &str, is_void: bool) -> u32 {
    if self.is_self_closing || is_void {
      self.loc.end.offset
    } else {
      element_close_span(source, self.loc.end.offset, self.tag.as_str()).end
    }
  }
}

/// Helpers over [`AttributeNode`].
pub trait AttributeExt {
  /// Full span of the `name` or `name="value"` form, with `loc.end` shifted
  /// past the closing quote when present.
  ///
  /// Vize emits `attr.loc.end` AT the closing quote (not past it), and for
  /// boolean attributes it may include trailing whitespace.
  fn full_span(&self, source: &str) -> Span;
}

impl AttributeExt for AttributeNode {
  fn full_span(&self, source: &str) -> Span {
    let start = self.loc.start.offset;
    let loc_end = self.loc.end.offset;
    let end = if self.value.is_some() && is_quote_byte(source, loc_end) {
      loc_end + 1
    } else {
      roffset(source, loc_end)
    };
    Span::new(start, end)
  }
}

/// Helpers over [`DirectiveNode`].
pub trait DirectiveExt {
  /// Full directive span with `loc.end` shifted past the closing quote.
  /// Equivalent to [`AttributeExt::full_span`] but for directives, which
  /// vize represents as a separate node type.
  fn full_span(&self, source: &str) -> Span;

  /// Span covering the directive *head* — everything up to (but excluding)
  /// the `=` sign, or the whole directive when there is no value.
  fn head_span(&self, source: &str) -> Span;
}

impl DirectiveExt for DirectiveNode<'_> {
  fn full_span(&self, source: &str) -> Span {
    let start = self.loc.start.offset;
    let loc_end = self.loc.end.offset;
    let end = if is_quote_byte(source, loc_end) { loc_end + 1 } else { roffset(source, loc_end) };
    Span::new(start, end)
  }

  fn head_span(&self, source: &str) -> Span {
    let start = self.loc.start.offset;
    let dir_text = self.loc.span().source_text(source);
    let head_end =
      dir_text.find('=').map_or_else(|| roffset(source, self.loc.end.offset), |i| start + i as u32);
    Span::new(start, head_end)
  }
}

/// True when the directive's argument is a static identifier (e.g. `:foo`).
pub fn is_static_arg(arg: &ExpressionNode<'_>) -> bool {
  match arg {
    ExpressionNode::Simple(s) => s.is_static,
    ExpressionNode::Compound(_) => false,
  }
}

/// True when the directive's argument is dynamic (e.g. `:[name]`).
pub fn is_dynamic_arg(arg: &ExpressionNode<'_>) -> bool {
  !is_static_arg(arg)
}

/// Find the closing tag span for an element given its opening tag end offset.
/// Scans forward through `source` tracking nesting depth to find the matching
/// `</tag>`.
///
/// vize's [`ElementNode::loc`] only covers the opening tag, so this walk is
/// necessary to recover the full element span. Returns an empty span at
/// `open_end` if no matching close is found (malformed input).
pub fn element_close_span(source: &str, open_end: u32, tag_name: &str) -> Span {
  let src = source.as_bytes();
  let tag_bytes = tag_name.as_bytes();
  let mut pos = open_end as usize;
  let mut depth = 1usize;

  while pos < src.len() {
    let Some(rel) = memchr::memchr(b'<', &src[pos..]) else { break };
    pos += rel;
    let rest = &src[pos + 1..];

    if rest.first() == Some(&b'/') {
      let after_slash = &rest[1..];
      if after_slash.starts_with(tag_bytes) {
        let after_name = tag_bytes.len();
        let ch = after_slash.get(after_name).copied().unwrap_or(b'>');
        if matches!(ch, b'>' | b' ' | b'\n' | b'\r' | b'\t') {
          depth -= 1;
          if depth == 0 {
            let gt = memchr::memchr(b'>', &src[pos..]).unwrap();
            return Span::new(pos as u32, (pos + gt + 1) as u32);
          }
        }
      }
    } else if rest.starts_with(tag_bytes) {
      let after_name = tag_bytes.len();
      let ch = rest.get(after_name).copied().unwrap_or(0);
      if matches!(ch, b'>' | b' ' | b'\n' | b'\r' | b'\t' | b'/') {
        depth += 1;
      }
    }
    pos += 1;
  }

  Span::new(open_end, open_end)
}

fn is_quote_byte(source: &str, offset: u32) -> bool {
  matches!(source.as_bytes().get(offset as usize), Some(&(b'"' | b'\'')))
}
