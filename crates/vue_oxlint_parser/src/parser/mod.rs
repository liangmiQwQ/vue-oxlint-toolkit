//! Vue SFC recursive-descent parser.
//!
//! Phase 1 sets up the public surface and module structure; phases 3 and 4
//! fill in the actual parsing logic.

mod script;
mod template;

use std::ptr;

use oxc_allocator::Allocator;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::ModuleRecord;
use rustc_hash::FxHashSet;

use crate::ast::VueSingleFileComponent;
use crate::lexer::Lexer;

/// Result of a Vue SFC parse.
///
/// Mirrors `oxc_parser::ParserReturn` in spirit: a single struct with the
/// parsed root, side-channel metadata, and a recoverable-vs-fatal split via
/// `errors` + `panicked`.
pub struct VueParserReturn<'a, 'b> {
  pub sfc: VueSingleFileComponent<'a, 'b>,
  pub irregular_whitespaces: Box<[Span]>,
  /// Spans coming directly from a single `oxc_parser` call — see the
  /// clean-codegen-mapping RFC for how the codegen side consumes this.
  pub clean_spans: FxHashSet<Span>,
  pub module_record: ModuleRecord<'b>,
  /// Tokens from the script side, produced by `oxc_parser` with
  /// [`oxc_parser::config::RuntimeParserConfig::new(true)`].
  pub script_tokens: oxc_allocator::Vec<'b, oxc_parser::Token>,
  /// Tokens from our first-party template lexer.
  pub template_tokens: oxc_allocator::Vec<'a, crate::lexer::VToken>,
  pub errors: Vec<OxcDiagnostic>,
  /// Set on unrecoverable structural errors (e.g. unclosed `<template>`).
  pub panicked: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VueParseConfig {
  /// Whether the consumer needs the parser to record `clean_spans`. The JSX
  /// crate sets this; the toolkit side doesn't need it.
  pub track_clean_spans: bool,
}

/// Vue SFC parser.
///
/// ## Lifetimes
///
/// - `'a` owns V-tree nodes (allocated in `allocator_a`).
/// - `'b` owns nodes produced by `oxc_parser` (allocated in `allocator_b`).
/// - `'b: 'a` — V-tree nodes may borrow from `oxc_parser` output, never the
///   reverse.
///
/// Two-allocator design is documented in the RFC; phase 1 wires the lifetime
/// plumbing without committing to its correctness — the open question is
/// flagged in the RFC.
#[expect(
  dead_code,
  reason = "phases 3-4 will read these fields; kept on the struct so the public surface is stable from phase 1"
)]
pub struct VueParser<'a, 'b>
where
  'b: 'a,
{
  allocator_a: &'a Allocator,
  allocator_b: &'b Allocator,
  origin_source_text: &'a str,

  options: ParseOptions,
  config: VueParseConfig,

  /// Mirror of [`crate::lexer::Lexer`]'s mutable buffer trick from the JSX
  /// crate — wrap bytes are written here, parsed via `oxc_parser`, then the
  /// buffer is reset to match `origin_source_text`.
  ///
  /// Spans on the resulting AST refer to original SFC offsets, not the
  /// rewritten buffer.
  source_text: &'a str,
  mut_ptr_source_text: *mut [u8],

  source_type: SourceType,
  errors: Vec<OxcDiagnostic>,
  clean_spans: FxHashSet<Span>,
}

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub fn new(
    allocator_a: &'a Allocator,
    allocator_b: &'b Allocator,
    source_text: &'a str,
    options: ParseOptions,
    config: VueParseConfig,
  ) -> Self {
    let alloced_str = allocator_a.alloc_slice_copy(source_text.as_bytes());

    Self {
      allocator_a,
      allocator_b,
      origin_source_text: source_text,
      options,
      config,

      mut_ptr_source_text: ptr::from_mut(alloced_str),
      // SAFETY: `alloced_str` was just copied from a `&str`.
      source_text: unsafe { str::from_utf8_unchecked(alloced_str) },

      source_type: SourceType::mjs().with_unambiguous(true),
      errors: Vec::new(),
      clean_spans: FxHashSet::default(),
    }
  }

  /// Parse the SFC. Phase 4 will implement this.
  #[must_use]
  pub fn parse(self) -> VueParserReturn<'a, 'b> {
    let _lexer = Lexer::new(self.allocator_a, self.source_text);
    todo!("phase 4: drive the lexer and recursive-descent parser")
  }

  /// Reset the mutable source buffer to match the original source.
  ///
  /// Called after each in-place wrap-and-parse cycle (see the RFC's
  /// "Reusing the `oxc_parse` mutation trick" section).
  pub const fn sync_source_text(&mut self) {
    // SAFETY: `self.origin_source_text` and `self.mut_ptr_source_text` have
    // identical lengths; the former lives on the heap and the latter in the
    // arena, so the regions cannot overlap.
    unsafe {
      ptr::copy_nonoverlapping(
        self.origin_source_text.as_ptr(),
        self.mut_ptr_source_text.cast(),
        self.origin_source_text.len(),
      );
    }
  }
}
