//! Vue SFC recursive-descent parser.
//!
//! Phase 1 sets up the public surface and module structure; phases 3 and 4
//! fill in the actual parsing logic.

mod script;
mod template;

use std::ptr;

use crate::lexer::Lexer;
use crate::{VueParser, VueParserReturn};

impl<'a, 'b: 'a> VueParser<'a, 'b> {
  /// Parse the SFC. Phase 4 will implement this.
  #[must_use]
  pub fn parse(self) -> VueParserReturn<'a, 'b> {
    let mut lexer = Lexer::new(self.vue_allocator, self.source_text);

    while let Some(token) = lexer.next_token() {
      todo!("token: {:p}", &token)
    }

    todo!()
  }

  /// Reset the mutable source buffer to match the original source.
  ///
  /// Called after each in-place wrap-and-parse cycle (see the RFC's
  /// "Reusing the `oxc_parse` mutation trick" section).
  const fn sync_source_text(&mut self) {
    // SAFETY: `self.origin_source_text` and `self.mut_ptr_oxc_source_text` have
    // identical lengths; the former lives on the heap and the latter in the
    // arena, so the regions cannot overlap.
    unsafe {
      ptr::copy_nonoverlapping(
        self.origin_source_text.as_ptr(),
        self.mut_ptr_oxc_source_text.cast(),
        self.origin_source_text.len(),
      );
    }
  }
}
