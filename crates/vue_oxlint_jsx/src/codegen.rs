use oxc_allocator::Allocator;
use oxc_ast::Comment;
use oxc_codegen::Codegen;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::{SourceType, Span};

use crate::parser::{ParseConfig, ParserImpl};

/// Configuration for [`VueOxcCodegen`].
///
/// The struct is `#[non_exhaustive]` so additional knobs (e.g. minification,
/// source map output) can be added in the future without a breaking change.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct CodegenMode;

impl CodegenMode {
  #[must_use]
  pub const fn new() -> Self {
    Self
  }
}

/// The return value of [`VueOxcCodegen::build`].
#[non_exhaustive]
pub struct VueOxcCodegenReturn {
  /// The generated JS/TS source produced from the Vue SFC.
  pub source_text: String,
  /// The detected source type (JSX vs TSX, module vs script).
  pub source_type: SourceType,
  /// Comments collected from the parsed source. Spans refer to the original
  /// Vue SFC source, not [`VueOxcCodegenReturn::source_text`].
  pub comments: Vec<Comment>,
  /// Irregular whitespace spans in the original Vue SFC source.
  pub irregular_whitespaces: Box<[Span]>,
  /// Diagnostics produced while parsing the Vue SFC.
  pub errors: Vec<OxcDiagnostic>,
  /// `true` if parsing fatally failed; [`VueOxcCodegenReturn::source_text`]
  /// will be empty in that case.
  pub panicked: bool,
}

/// Parses a Vue SFC and emits the resulting JS/TS source via `oxc_codegen`.
///
/// Unlike [`crate::VueOxcParser`] this entry point does not surface the AST
/// — the parser allocator lives only for the duration of [`Self::build`] and
/// is dropped before returning. Use this when you only need the generated
/// code (e.g. for downstream tooling that lints or transforms the output).
///
/// # Examples
///
/// ```
/// use vue_oxlint_jsx::{CodegenMode, VueOxcCodegen};
///
/// let source = r#"<template><div>{{ msg }}</div></template>
/// <script setup>
/// const msg = 'hello';
/// </script>"#;
///
/// let ret = VueOxcCodegen::new(source).build(CodegenMode::new());
/// assert!(!ret.panicked);
/// ```
pub struct VueOxcCodegen<'a> {
  source_text: &'a str,
  options: ParseOptions,
}

impl<'a> VueOxcCodegen<'a> {
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
  pub fn build(self, _mode: CodegenMode) -> VueOxcCodegenReturn {
    let allocator = Allocator::default();
    let ret =
      ParserImpl::new(&allocator, self.source_text, self.options, ParseConfig { codegen: true })
        .parse();

    if ret.fatal {
      return VueOxcCodegenReturn {
        source_text: String::new(),
        source_type: ret.program.source_type,
        comments: Vec::new(),
        irregular_whitespaces: Box::new([]),
        errors: ret.errors,
        panicked: true,
      };
    }

    let source_text = Codegen::new().build(&ret.program).code;
    let source_type = ret.program.source_type;
    let comments = ret.program.comments.iter().copied().collect();

    VueOxcCodegenReturn {
      source_text,
      source_type,
      comments,
      irregular_whitespaces: ret.irregular_whitespaces,
      errors: ret.errors,
      panicked: false,
    }
  }
}
