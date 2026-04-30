use oxc_allocator::{Allocator, Dummy};
use oxc_ast::ast::Program;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::Span;
use oxc_syntax::module_record::ModuleRecord;

use crate::parser::{ParseConfig, ParserImpl, ParserImplReturn};

pub struct VueJsxParser<'a> {
  allocator: &'a Allocator,
  source_text: &'a str,
  options: ParseOptions,
}

/// The return value of [`VueJsxParser::parse`].
///
/// Mirrors [`oxc_parser::ParserReturn`] as a workaround for its
/// `#[non_exhaustive]` attribute. The `is_flow_language` field is intentionally
/// omitted because Vue does not support Flow.
#[non_exhaustive]
pub struct VueJsxParserReturn<'a> {
  pub program: Program<'a>,
  pub module_record: ModuleRecord<'a>,
  pub errors: Vec<OxcDiagnostic>,
  pub irregular_whitespaces: Box<[Span]>,
  pub panicked: bool,
}

impl<'a> VueJsxParser<'a> {
  /// Creates a new [`VueJsxParser`] for the given Vue SFC `source_text`.
  ///
  /// The `allocator` must outlive the returned parser and the resulting
  /// [`VueJsxParserReturn`], because the produced AST nodes are arena-allocated.
  ///
  /// # Examples
  ///
  /// ```
  /// use oxc_allocator::Allocator;
  /// use vue_oxlint_jsx::VueJsxParser;
  ///
  /// let allocator = Allocator::default();
  /// let source = r#"<template><div>{{ msg }}</div></template>
  /// <script setup>
  /// const msg = 'hello';
  /// </script>"#;
  ///
  /// let ret = VueJsxParser::new(&allocator, source).parse();
  /// assert!(!ret.panicked);
  /// ```
  pub fn new(allocator: &'a Allocator, source_text: &'a str) -> Self {
    Self { allocator, source_text, options: ParseOptions::default() }
  }

  /// Overrides the [`ParseOptions`] passed to the underlying `oxc_parser`.
  ///
  /// # Examples
  ///
  /// ```
  /// use oxc_allocator::Allocator;
  /// use oxc_parser::ParseOptions;
  /// use vue_oxlint_jsx::VueJsxParser;
  ///
  /// let allocator = Allocator::default();
  /// let source = "<script setup lang=\"ts\">const n: number = 1;</script>";
  ///
  /// let options = ParseOptions { parse_regular_expression: true, ..ParseOptions::default() };
  /// let ret = VueJsxParser::new(&allocator, source).with_options(options).parse();
  /// assert!(!ret.panicked);
  /// ```
  #[must_use]
  pub const fn with_options(mut self, options: ParseOptions) -> Self {
    self.options = options;
    self
  }
}

impl<'a> VueJsxParser<'a> {
  /// Parses the Vue SFC and returns a [`VueJsxParserReturn`] containing the
  /// JS/TS [`Program`], the [`ModuleRecord`], collected diagnostics, and any
  /// irregular whitespace spans found in the source.
  ///
  /// On a fatal parse failure, [`VueJsxParserReturn::panicked`] is `true` and
  /// [`VueJsxParserReturn::program`] is a dummy program; callers should inspect
  /// [`VueJsxParserReturn::errors`] in that case.
  ///
  /// # Examples
  ///
  /// ```
  /// use oxc_allocator::Allocator;
  /// use vue_oxlint_jsx::VueJsxParser;
  ///
  /// let allocator = Allocator::default();
  /// let source = r#"<script setup>const count = 1;</script>"#;
  ///
  /// let ret = VueJsxParser::new(&allocator, source).parse();
  /// assert!(!ret.panicked);
  /// assert!(ret.errors.is_empty());
  /// ```
  #[must_use]
  pub fn parse(self) -> VueJsxParserReturn<'a> {
    let ParserImplReturn { program, errors, fatal, module_record, irregular_whitespaces } =
      ParserImpl::new(self.allocator, self.source_text, self.options, ParseConfig::default())
        .parse();

    if fatal {
      VueJsxParserReturn {
        program: Program::dummy(self.allocator),
        module_record, // Dummy one if fatal, can be directly passed there without recreate a new one
        errors,
        irregular_whitespaces: Box::new([]),
        panicked: true,
      }
    } else {
      VueJsxParserReturn { program, errors, panicked: false, irregular_whitespaces, module_record }
    }
  }
}
