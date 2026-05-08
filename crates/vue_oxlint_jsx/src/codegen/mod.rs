use oxc_allocator::Allocator;
use oxc_ast::Comment;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::{SourceType, Span};

use crate::parser::{ParseConfig, ParserImpl};

#[allow(
  clippy::branches_sharing_code,
  clippy::doc_markdown,
  clippy::missing_const_for_fn,
  clippy::option_if_let_else,
  unfulfilled_lint_expectations,
  clippy::useless_let_if_seq,
  clippy::wildcard_imports
)]
#[path = "oxc/lib.rs"]
mod oxc;

pub use self::oxc::{Codegen, Mapping};

/// The return value of [`VueJsxCodegen::build`].
#[non_exhaustive]
pub struct VueJsxCodegenReturn {
  /// The generated JS/TS source produced from the Vue SFC.
  pub source_text: String,
  /// The detected source type (JSX vs TSX, module vs script).
  pub source_type: SourceType,
  /// Comments collected from the parsed source. Spans refer to the original
  /// Vue SFC source, not [`VueJsxCodegenReturn::source_text`].
  pub comments: Vec<Comment>,
  /// Irregular whitespace spans in the original Vue SFC source.
  pub irregular_whitespaces: Box<[Span]>,
  /// Generated source ranges mapped back to original Vue SFC source ranges.
  pub mappings: Vec<Mapping>,
  /// Diagnostics produced while parsing the Vue SFC.
  pub errors: Vec<OxcDiagnostic>,
  /// `true` if parsing fatally failed; [`VueJsxCodegenReturn::source_text`]
  /// will be empty in that case.
  pub panicked: bool,
}

/// Parses a Vue SFC and emits the resulting JS/TS source via `oxc_codegen`.
///
/// Unlike [`crate::VueJsxParser`] this entry point does not surface the AST
/// — the parser allocator lives only for the duration of [`Self::build`] and
/// is dropped before returning. Use this when you only need the generated
/// code (e.g. for downstream tooling that lints or transforms the output).
///
/// # Examples
///
/// ```
/// use vue_oxlint_jsx::VueJsxCodegen;
///
/// let source = r#"<template><div>{{ msg }}</div></template>
/// <script setup>
/// const msg = 'hello';
/// </script>"#;
///
/// let ret = VueJsxCodegen::new(source).build();
/// assert!(!ret.panicked);
/// ```
pub struct VueJsxCodegen<'a> {
  source_text: &'a str,
  options: ParseOptions,
}

impl<'a> VueJsxCodegen<'a> {
  #[must_use]
  pub fn new(source_text: &'a str) -> Self {
    Self { source_text, options: ParseOptions::default() }
  }

  /// Overrides the [`ParseOptions`] passed to the underlying `oxc_parser`.
  #[must_use]
  pub const fn with_options(mut self, options: ParseOptions) -> Self {
    self.options = options;
    self
  }

  /// Parses the Vue SFC and runs `oxc_codegen` to produce JS/TS source.
  #[must_use]
  pub fn build(self) -> VueJsxCodegenReturn {
    let allocator = Allocator::default();
    let ret =
      ParserImpl::new(&allocator, self.source_text, self.options, ParseConfig { codegen: true })
        .parse();

    if ret.fatal {
      return VueJsxCodegenReturn {
        source_text: String::new(),
        source_type: ret.program.source_type,
        comments: Vec::new(),
        irregular_whitespaces: Box::new([]),
        mappings: Vec::new(),
        errors: ret.errors,
        panicked: true,
      };
    }

    let codegen_ret = Codegen::new().with_clean_spans(ret.clean_spans).build(&ret.program);
    let source_text = codegen_ret.code;
    let source_type = ret.program.source_type;
    let comments = ret.program.comments.iter().copied().collect();

    VueJsxCodegenReturn {
      source_text,
      source_type,
      comments,
      irregular_whitespaces: ret.irregular_whitespaces,
      mappings: codegen_ret.mappings,
      errors: ret.errors,
      panicked: false,
    }
  }
}
