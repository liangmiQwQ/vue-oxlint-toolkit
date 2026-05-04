use std::ptr;

use oxc_ast::Comment;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::{ParseOptions, Token};
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::ModuleRecord;
use rustc_hash::FxHashSet;

use crate::ast::VueSingleFileComponent;
use oxc_allocator::{Allocator, Vec as ArenaVec};

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
  origin_source_text: &'a str,

  /// Template-side source used by the lexer and recursive-descent parser.
  source_text: &'a str,
  options: ParseOptions,

  /// Mirror of the JSX crate's mutable buffer trick for `oxc_parser` calls:
  /// wrap bytes are written here, parsed, then reset to match
  /// `origin_source_text`.
  ///
  /// Spans on the resulting AST refer to original SFC offsets, not the
  /// rewritten buffer.
  oxc_source_text: &'b str,
  mut_ptr_oxc_source_text: *mut [u8],

  source_type: SourceType,
  errors: Vec<OxcDiagnostic>,
  clean_spans: FxHashSet<Span>,
  script_comments: ArenaVec<'a, Comment>,
  script_tokens: ArenaVec<'b, Token>,

  module_record: ModuleRecord<'b>,
  script_lang: Option<&'a str>,

  script_set: bool,
  script_setup_set: bool,
}

impl<'a, 'b> VueParser<'a, 'b> {
  pub fn new(
    vue_allocator: &'a Allocator,
    js_allocator: &'b Allocator,
    source_text: &'a str,
  ) -> Self {
    let alloced_str_a = vue_allocator.alloc_slice_copy(source_text.as_bytes());
    let alloced_str_b = js_allocator.alloc_slice_copy(source_text.as_bytes());

    Self {
      vue_allocator,
      js_allocator,
      origin_source_text: source_text,

      // SAFETY: both slices were copied from a `&str`.
      source_text: unsafe { str::from_utf8_unchecked(alloced_str_a) },
      options: ParseOptions::default(),

      mut_ptr_oxc_source_text: ptr::from_mut(alloced_str_b),
      oxc_source_text: unsafe { str::from_utf8_unchecked(alloced_str_b) },

      source_type: SourceType::mjs().with_unambiguous(true),
      errors: Vec::new(),
      clean_spans: FxHashSet::default(),
      module_record: ModuleRecord::new(js_allocator),

      script_comments: ArenaVec::new_in(js_allocator),
      script_tokens: ArenaVec::new_in(js_allocator),

      script_lang: None,
      script_set: false,
      script_setup_set: false,
    }
  }

  #[must_use]
  pub const fn with_options(mut self, options: ParseOptions) -> Self {
    self.options = options;
    self
  }
}
