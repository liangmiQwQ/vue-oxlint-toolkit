//! Core parser implementation for Vue SFCs.

use std::ptr;

use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_ast::{
  AstBuilder, Comment,
  ast::{Directive, Program, Statement},
};
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::{ParseOptions, Token, config::TokensParserConfig};
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::ModuleRecord;
use rustc_hash::FxHashSet;

pub mod attribute;
pub mod element;
pub mod expression;
pub mod lexer;
pub mod script;

use crate::ast::VueSingleFileComponent;
use crate::irregular_whitespaces::collect_irregular_whitespaces;

/// Internal parser state
pub struct Parser<'a> {
  pub allocator: &'a Allocator,
  pub source_text: &'a str,
  /// Arena-allocated mutable copy of source bytes (used for wrap-and-parse trick)
  pub mut_ptr_source_text: *mut [u8],
  /// The mutable slice as a str reference (always in sync with origin unless mid-parse)
  pub source_str: &'a str,
  pub ast: AstBuilder<'a>,

  pub source_type: SourceType,
  pub module_record: ModuleRecord<'a>,
  pub script_comments: Vec<Comment>,
  /// JS tokens from `<script>` / `<script setup>` bodies, collected via
  /// `TokensParserConfig`. Consumers need these to produce `vue-eslint-parser`-shaped
  /// `Program.tokens` arrays.
  pub script_tokens: Vec<Token>,
  pub clean_spans: FxHashSet<Span>,
  pub errors: Vec<OxcDiagnostic>,

  /// Current byte position in `source_text`
  pub pos: usize,

  /// Whether we've already seen `<script>`
  pub script_set: bool,
  /// Whether we've already seen `<script setup>`
  pub setup_set: bool,

  pub panicked: bool,
}

impl<'a> Parser<'a> {
  pub fn new(allocator: &'a Allocator, source_text: &'a str) -> Self {
    let ast = AstBuilder::new(allocator);
    let alloced = allocator.alloc_slice_copy(source_text.as_bytes());
    let mut_ptr = ptr::from_mut(alloced);
    // SAFETY: alloced was copied from a valid &str, and we hold both
    // the raw mutable pointer AND an immutable reference; we ensure
    // they are never simultaneously used for mutation + reading via
    // the sync_source_text / oxc_parse_with_wrap discipline.
    let source_str = unsafe { str::from_utf8_unchecked(&*mut_ptr) };

    Self {
      allocator,
      source_text,
      mut_ptr_source_text: mut_ptr,
      source_str,
      ast,
      source_type: SourceType::mjs().with_unambiguous(true),
      module_record: ModuleRecord::new(allocator),
      script_comments: Vec::new(),
      script_tokens: Vec::new(),
      clean_spans: FxHashSet::default(),
      errors: Vec::new(),
      pos: 0,
      script_set: false,
      setup_set: false,
      panicked: false,
    }
  }

  /// Restore the arena copy to original source bytes
  pub const fn sync_source_text(&mut self) {
    // SAFETY: source_text and mut_ptr_source_text have same length, no overlap
    unsafe {
      ptr::copy_nonoverlapping(
        self.source_text.as_ptr(),
        self.mut_ptr_source_text.cast::<u8>(),
        self.source_text.len(),
      );
    }
  }

  /// The wrap-and-parse trick.
  ///
  /// Given a span `[start, end)` in the original source, this function:
  /// 1. Writes `start_wrap` into the bytes just before `start`
  /// 2. Writes `end_wrap` into the bytes starting at `end`
  /// 3. Pads everything before `start - start_wrap.len()` with spaces
  /// 4. Parses with `oxc_parser` (into `self.allocator`)
  /// 5. Resets the buffer back to original
  ///
  /// The resulting AST spans point at original SFC offsets.
  ///
  /// # Safety
  /// Caller must ensure:
  /// - There are at least `start_wrap.len()` bytes before `span.start` in the buffer
  /// - There are at least `end_wrap.len()` bytes after `span.end`
  pub unsafe fn oxc_parse_with_wrap(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
  ) -> Option<(ArenaVec<'a, Directive<'a>>, ArenaVec<'a, Statement<'a>>, ModuleRecord<'a>)> {
    let start = span.start as usize;
    let end = span.end as usize;

    unsafe {
      let real_start = start - start_wrap.len();
      let first_byte_ptr = self.mut_ptr_source_text.cast::<u8>();

      // Write start_wrap just before `start`
      ptr::copy_nonoverlapping(
        start_wrap.as_ptr(),
        first_byte_ptr.add(real_start),
        start_wrap.len(),
      );
      // Write end_wrap just after `end`
      ptr::copy_nonoverlapping(end_wrap.as_ptr(), first_byte_ptr.add(end), end_wrap.len());

      // Pad everything before real_start with spaces
      for i in 0..real_start {
        first_byte_ptr.add(i).write(b' ');
      }
    }

    // SAFETY: valid utf-8 (we only wrote ASCII bytes and spaces)
    let slice =
      unsafe { str::from_utf8_unchecked(&self.source_str.as_bytes()[..end + end_wrap.len()]) };

    // We need to extend the slice lifetime to 'a to satisfy the arena borrow.
    // SAFETY: `self.source_str` points to arena-allocated memory whose lifetime is 'a,
    // so this cast is sound as long as we reset before returning.
    let slice: &'a str = unsafe { &*std::ptr::from_ref::<str>(slice) };

    let allocator = self.allocator;
    let result = self.call_oxc_parse(slice, allocator);

    // Reset to original
    self.sync_source_text();
    result
  }

  /// Parse a raw script slice directly (no wrap).
  /// Tokens are collected via [`TokensParserConfig`] and appended to
  /// `self.script_tokens` so downstream consumers can populate
  /// `vue-eslint-parser`-shaped `Program.tokens`.
  ///
  /// Returns `(Program, ModuleRecord)`.
  pub fn oxc_parse_script(&mut self, span: Span) -> Option<(Program<'a>, ModuleRecord<'a>)> {
    let start = span.start as usize;
    let end = span.end as usize;

    // Pad everything before `start` with spaces so offsets align
    unsafe {
      let first_byte_ptr = self.mut_ptr_source_text.cast::<u8>();
      for i in 0..start {
        first_byte_ptr.add(i).write(b' ');
      }
    }

    // SAFETY: valid utf-8; extend lifetime to 'a (arena allocation)
    let slice: &'a str = unsafe {
      &*std::ptr::from_ref::<str>(str::from_utf8_unchecked(&self.source_str.as_bytes()[..end]))
    };

    let allocator = self.allocator;
    let mut ret = oxc_parser::Parser::new(allocator, slice, self.source_type)
      .with_config(TokensParserConfig)
      .with_options(ParseOptions { parse_regular_expression: true, ..ParseOptions::default() })
      .parse();

    self.errors.append(&mut ret.errors);

    // Reset
    self.sync_source_text();

    if ret.panicked {
      None
    } else {
      self.script_comments.extend(ret.program.comments.iter().copied());
      // Collect tokens — copy out of the arena Vec into an owned Vec.
      // Token is Copy (a u128) so this is cheap.
      self.script_tokens.extend(ret.tokens.iter().copied());
      Some((ret.program, ret.module_record))
    }
  }

  fn call_oxc_parse(
    &mut self,
    source: &'a str,
    allocator: &'a Allocator,
  ) -> Option<(ArenaVec<'a, Directive<'a>>, ArenaVec<'a, Statement<'a>>, ModuleRecord<'a>)> {
    let mut ret = oxc_parser::Parser::new(allocator, source, self.source_type)
      .with_options(ParseOptions { parse_regular_expression: true, ..ParseOptions::default() })
      .parse();

    self.errors.append(&mut ret.errors);

    if ret.panicked {
      None
    } else {
      self.script_comments.extend(ret.program.comments.iter().copied());
      Some((ret.program.directives, ret.program.body, ret.module_record))
    }
  }
}

/// Public parse entry point
pub fn parse_impl<'a>(
  allocator: &'a Allocator,
  source_text: &'a str,
) -> VueSingleFileComponent<'a> {
  let mut parser = Parser::new(allocator, source_text);

  let children = parser.parse_children(None);
  parser.sort_script_comments();

  let irregular_whitespaces = collect_irregular_whitespaces(source_text);
  let panicked = parser.panicked;

  VueSingleFileComponent {
    children,
    script_comments: parser.script_comments,
    script_tokens: parser.script_tokens,
    irregular_whitespaces,
    clean_spans: parser.clean_spans,
    module_record: parser.module_record,
    source_type: parser.source_type,
    errors: parser.errors,
    panicked,
  }
}

impl Parser<'_> {
  fn sort_script_comments(&mut self) {
    self.script_comments.sort_by_key(|c| c.span.start);
    self
      .errors
      .sort_by_key(|e| e.labels.as_ref().and_then(|l| l.first()).map_or(0, |l| l.offset() as u32));
  }
}
