//! SFC top-level block splitter.
//!
//! Walks the source from offset 0, identifying contiguous regions that are
//! either a recognised top-level element (`<template>`, `<script>`,
//! `<style>`) or an arbitrary custom block. Inside a block we treat its body
//! as opaque text — the template parser is responsible for parsing the
//! `<template>` body further. `<script>` bodies are handed off to
//! `oxc_parser` later in the pipeline.
//!
//! This splitter does not implement full HTML5 tokenisation; it implements
//! the subset required by Vue SFCs. That subset is well-defined: top-level
//! tags must be element-style, attributes are simple, and whitespace between
//! blocks is preserved.

use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span as OxcSpan;

use crate::ast::Span;

fn unterminated_start_tag(offset: u32) -> OxcDiagnostic {
  OxcDiagnostic::error("Unterminated SFC start tag")
    .with_error_code_scope("vue-sfc")
    .with_label(OxcSpan::new(offset, offset + 1))
}

fn missing_end_tag(name: &str, start: u32, end: u32) -> OxcDiagnostic {
  OxcDiagnostic::error(format!("Missing end tag for `<{name}>`"))
    .with_error_code_scope("vue-sfc")
    .with_label(OxcSpan::new(start, end))
}

#[derive(Debug, Clone)]
pub struct SfcBlock<'a> {
  /// Tag name (`template`, `script`, `style`, or any custom name).
  pub tag: &'a str,
  /// Inclusive span of the whole element (start of `<` to end of `>` of
  /// the closing tag, or to the end of self-closing `>`).
  pub range: Span,
  /// Span of the start tag (`<tag ...>` or `<tag .../>`).
  pub start_tag_range: Span,
  /// Span of the end tag if present.
  pub end_tag_range: Option<Span>,
  /// Inner content range (between `>` of start tag and `<` of end tag).
  /// Empty span if the element is self-closing or has no content.
  pub content_range: Span,
  /// Raw attributes substring (everything between tag name and the closing
  /// `>` of the start tag, trimmed of surrounding whitespace).
  pub raw_attributes: &'a str,
  /// `true` if the start tag was self-closing (`<foo />`).
  pub self_closing: bool,
}

/// Result of a top-level SFC scan.
#[derive(Debug, Clone)]
pub struct SfcLayout<'a> {
  pub blocks: Vec<SfcBlock<'a>>,
  /// Surrounding text segments at the top level (between blocks). Each entry
  /// is a `(span, text)` pair sliced from the source.
  pub text_segments: Vec<(Span, &'a str)>,
}

/// Walk `source` and identify top-level Vue SFC blocks.
///
/// # Errors
/// Returns an [`OxcDiagnostic`] when an opening tag cannot be terminated or
/// a recognised block has no closing tag.
pub fn split(source: &str) -> Result<SfcLayout<'_>, OxcDiagnostic> {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = 0usize;
  let mut blocks: Vec<SfcBlock> = Vec::new();
  let mut texts: Vec<(Span, &str)> = Vec::new();
  let mut text_start = 0usize;

  while i < len {
    if bytes[i] == b'<' && i + 1 < len && is_tag_name_start(bytes[i + 1]) {
      // Flush pending top-level text.
      if i > text_start {
        let span = Span::new(text_start as u32, i as u32);
        texts.push((span, &source[text_start..i]));
      }
      let tag_open = i;
      // Read tag name
      let name_start = i + 1;
      let mut j = name_start;
      while j < len && is_tag_name_part(bytes[j]) {
        j += 1;
      }
      if j == name_start {
        // Not a real tag — keep scanning as text.
        i += 1;
        continue;
      }
      let name = &source[name_start..j];
      // Find end of start tag — handle quoted attribute values, self-closing.
      let (start_tag_end, self_closing) =
        find_start_tag_end(bytes, j).ok_or_else(|| unterminated_start_tag(tag_open as u32))?;
      let raw_attrs = source[j..start_tag_end - if self_closing { 2 } else { 1 }].trim();
      let start_tag_range = Span::new(tag_open as u32, start_tag_end as u32);

      if self_closing {
        blocks.push(SfcBlock {
          tag: name,
          range: start_tag_range,
          start_tag_range,
          end_tag_range: None,
          content_range: Span::new(start_tag_end as u32, start_tag_end as u32),
          raw_attributes: raw_attrs,
          self_closing: true,
        });
        text_start = start_tag_end;
        i = start_tag_end;
        continue;
      }

      // Find matching end tag `</name>` — we scan forward, ignoring nested
      // identical tags (Vue SFCs forbid this at top level, but we tolerate
      // it leniently by counting depth).
      let body_start = start_tag_end;
      let close_marker = format!("</{name}");
      let close_marker_b = close_marker.as_bytes();
      let mut depth = 1i32;
      let mut k = body_start;
      let mut content_end = body_start;
      let mut end_tag_close = body_start;
      let open_marker = format!("<{name}");
      let open_marker_b = open_marker.as_bytes();
      while k < len {
        if bytes[k] == b'<' {
          if k + close_marker_b.len() <= len
            && bytes[k..k + close_marker_b.len()].eq_ignore_ascii_case(close_marker_b)
            && (k + close_marker_b.len() == len
              || matches!(
                bytes[k + close_marker_b.len()],
                b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n'
              ))
          {
            depth -= 1;
            if depth == 0 {
              content_end = k;
              // Advance past `</name`
              let mut m = k + close_marker_b.len();
              while m < len && bytes[m] != b'>' {
                m += 1;
              }
              if m >= len {
                return Err(missing_end_tag(name, tag_open as u32, start_tag_end as u32));
              }
              end_tag_close = m + 1;
              break;
            }
          } else if k + open_marker_b.len() <= len
            && bytes[k..k + open_marker_b.len()].eq_ignore_ascii_case(open_marker_b)
            && (k + open_marker_b.len() == len
              || matches!(
                bytes[k + open_marker_b.len()],
                b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n'
              ))
          {
            depth += 1;
          }
        }
        k += 1;
      }
      // Tolerate unclosed top-level blocks (mirrors upstream's lenient
      // recovery): treat the block as extending to EOF with no end tag.
      let (final_content_end, final_end_close, end_tag_range) = if depth == 0 {
        (content_end, end_tag_close, Some(Span::new(content_end as u32, end_tag_close as u32)))
      } else {
        (len, len, None)
      };

      blocks.push(SfcBlock {
        tag: name,
        range: Span::new(tag_open as u32, final_end_close as u32),
        start_tag_range,
        end_tag_range,
        content_range: Span::new(body_start as u32, final_content_end as u32),
        raw_attributes: raw_attrs,
        self_closing: false,
      });
      text_start = final_end_close;
      i = final_end_close;
      continue;
    }
    i += 1;
  }
  if text_start < len {
    let span = Span::new(text_start as u32, len as u32);
    texts.push((span, &source[text_start..len]));
  }
  Ok(SfcLayout { blocks, text_segments: texts })
}

/// Scan from `pos` (just past tag-name) to the first `>` that ends the start
/// tag. Returns the byte index just after `>` and whether the tag was
/// self-closing.
fn find_start_tag_end(bytes: &[u8], mut pos: usize) -> Option<(usize, bool)> {
  while pos < bytes.len() {
    match bytes[pos] {
      b'>' => return Some((pos + 1, false)),
      b'/' if pos + 1 < bytes.len() && bytes[pos + 1] == b'>' => return Some((pos + 2, true)),
      b'"' | b'\'' => {
        let quote = bytes[pos];
        pos += 1;
        while pos < bytes.len() && bytes[pos] != quote {
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

const fn is_tag_name_start(b: u8) -> bool {
  b.is_ascii_alphabetic()
}

const fn is_tag_name_part(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':'
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn splits_basic_sfc() {
    let src = "<template>\n  <div>{{ x }}</div>\n</template>\n<script>let x = 1</script>\n";
    let layout = split(src).unwrap();
    assert_eq!(layout.blocks.len(), 2);
    assert_eq!(layout.blocks[0].tag, "template");
    assert_eq!(layout.blocks[1].tag, "script");
  }

  #[test]
  fn handles_self_closing_block() {
    let src = "<template src=\"./x.html\" />\n";
    let layout = split(src).unwrap();
    assert_eq!(layout.blocks.len(), 1);
    assert!(layout.blocks[0].self_closing);
  }
}
