use oxc_allocator::{Allocator, Dummy};
use oxc_ast::ast::Program;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::Span;
use oxc_syntax::module_record::ModuleRecord;

use crate::parser::{ParserImpl, ParserImplReturn};

mod irregular_whitespaces;
mod parser;

#[cfg(test)]
mod test;

pub struct VueOxcParser<'a> {
  allocator: &'a Allocator,
  source_text: &'a str,
  options: ParseOptions,
}

/// The return value of [`VueOxcParser::parse`].
///
/// Mirrors [`oxc_parser::ParserReturn`] as a workaround for its
/// `#[non_exhaustive]` attribute. The `is_flow_language` field is intentionally
/// omitted because Vue does not support Flow.
#[non_exhaustive]
pub struct VueParserReturn<'a> {
  pub program: Program<'a>,
  pub module_record: ModuleRecord<'a>,
  pub errors: Vec<OxcDiagnostic>,
  pub irregular_whitespaces: Box<[Span]>,
  pub panicked: bool,
}

impl<'a> VueOxcParser<'a> {
  /// Creates a new [`VueOxcParser`] for the given Vue SFC `source_text`.
  ///
  /// The `allocator` must outlive the returned parser and the resulting
  /// [`VueParserReturn`], because the produced AST nodes are arena-allocated.
  ///
  /// # Examples
  ///
  /// ```
  /// use oxc_allocator::Allocator;
  /// use vue_oxlint_jsx::VueOxcParser;
  ///
  /// let allocator = Allocator::default();
  /// let source = r#"<template><div>{{ msg }}</div></template>
  /// <script setup>
  /// const msg = 'hello';
  /// </script>"#;
  ///
  /// let ret = VueOxcParser::new(&allocator, source).parse();
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
  /// use vue_oxlint_jsx::VueOxcParser;
  ///
  /// let allocator = Allocator::default();
  /// let source = "<script setup lang=\"ts\">const n: number = 1;</script>";
  ///
  /// let options = ParseOptions { parse_regular_expression: true, ..ParseOptions::default() };
  /// let ret = VueOxcParser::new(&allocator, source).with_options(options).parse();
  /// assert!(!ret.panicked);
  /// ```
  #[must_use]
  pub const fn with_options(mut self, options: ParseOptions) -> Self {
    self.options = options;
    self
  }
}

impl<'a> VueOxcParser<'a> {
  /// Parses the Vue SFC and returns a [`VueParserReturn`] containing the
  /// JS/TS [`Program`], the [`ModuleRecord`], collected diagnostics, and any
  /// irregular whitespace spans found in the source.
  ///
  /// On a fatal parse failure, [`VueParserReturn::panicked`] is `true` and
  /// [`VueParserReturn::program`] is a dummy program; callers should inspect
  /// [`VueParserReturn::errors`] in that case.
  ///
  /// # Examples
  ///
  /// ```
  /// use oxc_allocator::Allocator;
  /// use vue_oxlint_jsx::VueOxcParser;
  ///
  /// let allocator = Allocator::default();
  /// let source = r#"<script setup>const count = 1;</script>"#;
  ///
  /// let ret = VueOxcParser::new(&allocator, source).parse();
  /// assert!(!ret.panicked);
  /// assert!(ret.errors.is_empty());
  /// ```
  #[must_use]
  pub fn parse(self) -> VueParserReturn<'a> {
    let ParserImplReturn { program, errors, fatal, module_record } =
      ParserImpl::new(self.allocator, self.source_text, self.options).parse();

    if fatal {
      VueParserReturn {
        program: Program::dummy(self.allocator),
        module_record, // Dummy one if fatal, can be directly passed there without recreate a new one
        errors,
        irregular_whitespaces: Box::new([]),
        panicked: true,
      }
    } else {
      VueParserReturn {
        program,
        errors,
        panicked: false,
        irregular_whitespaces: self.get_irregular_whitespaces(),
        module_record,
      }
    }
  }
}
