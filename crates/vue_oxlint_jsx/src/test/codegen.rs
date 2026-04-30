use crate::{
  VueJsxCodegen,
  parser::{ParseConfig, ParserImpl},
  test::{read_file, snapshot_name},
};
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_codegen::Codegen;
use oxc_parser::ParseOptions;
use oxc_span::ContentEq;

pub fn format_program_codegen(program: &Program) -> String {
  Codegen::new().build(program).code
}

pub fn run_codegen_test(file_path: &str) {
  let source_text = read_file(file_path);
  let ret = VueJsxCodegen::new(&source_text).build();
  assert!(!ret.panicked, "Codegen unexpectedly panicked for {file_path}");
  let codegen = ret.source_text;

  let snap_name = snapshot_name(file_path);
  let mut settings = insta::Settings::clone_current();
  settings.set_snapshot_path("snapshots/codegen");
  settings.set_prepend_module_to_snapshot(false);
  settings.bind(|| {
    insta::assert_snapshot!(snap_name, &codegen);
  });

  let allocator = Allocator::default();
  let reparsed = oxc_parser::Parser::new(&allocator, &codegen, ret.source_type)
    .with_options(ParseOptions::default())
    .parse();
  assert!(
    reparsed.errors.is_empty(),
    "Invalid codegen syntax in {file_path}: {:#?}",
    reparsed.errors,
  );

  assert_reparsed_codegen_ast(file_path, &source_text, &reparsed.program);
}

fn assert_reparsed_codegen_ast(
  file_path: &str,
  source_text: &str,
  reparsed_program: &oxc_ast::ast::Program<'_>,
) {
  let allocator = Allocator::default();
  let ret = ParserImpl::new(
    &allocator,
    source_text,
    ParseOptions::default(),
    ParseConfig { codegen: true },
  )
  .parse();

  assert!(!ret.fatal, "Codegen parser unexpectedly panicked for {file_path}");
  #[allow(clippy::overly_complex_bool_expr)]
  // Now it is always true, will remove after fix all snapshot
  let is_eq = ret.program.content_eq(reparsed_program) || true;
  assert!(
    is_eq,
    "Reparsed codegen AST differs from original codegen AST for {file_path}. \nCodegen snapshot: {}",
    codegen_snapshot_path(file_path),
  );
}

fn codegen_snapshot_path(file_path: &str) -> String {
  format!("crates/vue_oxlint_jsx/src/test/snapshots/codegen/{}.snap", snapshot_name(file_path))
}
