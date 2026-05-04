use std::ptr;

use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::ModuleRecord;
use rustc_hash::FxHashSet;

use crate::ast::VueSingleFileComponent;
use oxc_allocator::Allocator;

pub mod ast;
pub mod lexer;
pub mod parser;

/// Result of a Vue SFC parse.
///
/// Mirrors `oxc_parser::ParserReturn` in spirit: a single struct with the
/// parsed root, side-channel metadata, and a recoverable-vs-fatal split via
/// `errors` + `panicked`.
pub struct VueParserReturn<'a, 'b> {
  pub sfc: VueSingleFileComponent<'a, 'b>,
  pub irregular_whitespaces: Box<[Span]>,
  pub clean_spans: FxHashSet<Span>,
  pub module_record: ModuleRecord<'b>,
  pub errors: Vec<OxcDiagnostic>,
  pub panicked: bool,
}

/// Vue SFC parser.
///
/// ## Lifetimes
///
/// - `'a` owns V-tree nodes (allocated in `allocator_a`).
/// - `'b` owns nodes produced by `oxc_parser` (allocated in `allocator_b`).
/// - `'b: 'a` — V-tree nodes may borrow from `oxc_parser` output, never the reverse.
#[allow(dead_code)]
pub struct VueParser<'a, 'b>
where
  'b: 'a,
{
  vue_allocator: &'a Allocator,
  js_allocator: &'b Allocator,

  origin_source_text: &'b str,
  options: ParseOptions,

  /// This `source_text` may be changed as we define `mut_ptr_oxc_source_text`.
  /// This is a trick to reduce memory allocator and avoid creating a new `&str` in allocator.
  source_text: &'b str,
  mut_ptr_oxc_source_text: *mut [u8],

  errors: Vec<OxcDiagnostic>,
  source_type: SourceType,

  clean_spans: FxHashSet<Span>,
}

impl<'a, 'b: 'a> VueParser<'a, 'b> {
  pub fn new(
    vue_allocator: &'a Allocator,
    js_allocator: &'b Allocator,
    source_text: &'b str,
  ) -> Self {
    let alloced_str = js_allocator.alloc_slice_copy(source_text.as_bytes());

    Self {
      vue_allocator,
      js_allocator,

      // SAFETY: both slices were copied from a `&str`.
      origin_source_text: source_text,
      options: ParseOptions::default(),

      mut_ptr_oxc_source_text: ptr::from_mut(alloced_str),
      source_text: unsafe { str::from_utf8_unchecked(alloced_str) },

      source_type: SourceType::mjs().with_unambiguous(true),
      errors: Vec::new(),

      clean_spans: FxHashSet::default(),
    }
  }

  #[must_use]
  pub const fn with_options(mut self, options: ParseOptions) -> Self {
    self.options = options;
    self
  }
}
