//! Vue template lexer.
//!
//! Spans are byte offsets in the original SFC. The parser drives raw-text and
//! `v-pre` modes after it has consumed start tags.

mod data;
mod tag;
mod text;
mod tokens;
mod utils;

pub use tokens::{VToken, VTokenKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexerMode<'s> {
  Data,
  RawText(&'s str),
  RcData(&'s str),
}

/// Vue template lexer.
#[derive(Clone)]
pub struct Lexer<'s> {
  pub(super) source_text: &'s str,
  pub(super) source: &'s [u8],
  pub(super) pos: u32,
  pub(super) in_tag: bool,
  pub(super) interpolation: bool,
  pub(super) v_pre_depth: u32,
  pub(super) mode: LexerMode<'s>,
  panicked: bool,
}

impl<'s> Lexer<'s> {
  #[must_use]
  pub const fn new(source_text: &'s str) -> Self {
    Self {
      source_text,
      source: source_text.as_bytes(),
      pos: 0,
      in_tag: false,
      interpolation: false,
      v_pre_depth: 0,
      mode: LexerMode::Data,
      panicked: false,
    }
  }

  #[must_use]
  pub const fn panicked(&self) -> bool {
    self.panicked
  }

  pub const fn set_raw_text_mode(&mut self, tag_name: &'s str) {
    self.mode = LexerMode::RawText(tag_name);
  }

  pub const fn set_rc_data_mode(&mut self, tag_name: &'s str) {
    self.mode = LexerMode::RcData(tag_name);
  }

  pub const fn enter_v_pre(&mut self) {
    self.v_pre_depth += 1;
  }

  pub const fn leave_v_pre(&mut self) {
    self.v_pre_depth = self.v_pre_depth.saturating_sub(1);
  }

  pub const fn jump_to_eof(&mut self) {
    self.pos = self.source.len() as u32;
    self.panicked = true;
  }

  pub fn next_token(&mut self) -> Option<VToken<'s>> {
    if self.pos as usize >= self.source.len() {
      return None;
    }

    if self.in_tag {
      return Some(self.next_tag_token());
    }

    if self.interpolation {
      return Some(self.next_interpolation_token());
    }

    match self.mode {
      LexerMode::Data => Some(self.next_data_token()),
      LexerMode::RawText(tag_name) => Some(self.next_raw_text_token(tag_name)),
      LexerMode::RcData(tag_name) => Some(self.next_rc_data_token(tag_name)),
    }
  }
}
