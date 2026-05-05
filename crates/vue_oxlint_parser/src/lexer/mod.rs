//! Vue template lexer.
//!
//! HTML5-aware tokenizer that follows `vue-eslint-parser`'s behaviour:
//!
//! - Raw-text mode for `<script>`, `<style>`, `<xmp>`, `<noframes>`, `<noscript>`, `<noembed>`, `<iframe>`, `<plaintext>` — only the matching close tag terminates the body.
//! - RCDATA mode for `<textarea>` and `<title>` — the body is text but character references resolve.
//! - Foreign-content mode for `<svg>` / `<math>` — `<![CDATA[ ... ]]>` is recognised inside.
//! - `v-pre` mode where `{{` / `}}` is treated as text rather than as interpolation delimiters.
//!
//! The mode is set explicitly by the parser via [`Lexer::set_mode`] when it
//! crosses element boundaries — the lexer does not infer it from the tag
//! name on its own. This matches how `vue-eslint-parser` drives its
//! intermediate tokenizer.
//!
//! Spans are all in original SFC byte-offset space.

mod tokens;

pub use tokens::VToken;

use oxc_allocator::Allocator;

/// Vue template lexer.
///
/// Produces [`VToken`]s in source order with original-source spans.
#[allow(dead_code)]
pub struct Lexer<'a> {
  allocator: &'a Allocator,
  source: &'a [u8],
  pos: u32,
}

#[allow(dead_code)]
impl<'a> Lexer<'a> {
  #[must_use]
  pub const fn new(allocator: &'a Allocator, source_text: &'a str) -> Self {
    Self { allocator, source: source_text.as_bytes(), pos: 0 }
  }

  pub fn next_token(&mut self) -> Option<VToken<'a>> {
    self.pos = 10000;
    todo!("{self:p}")
  }
}
