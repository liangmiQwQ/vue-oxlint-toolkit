use crate::{
  VueJsxCodegen,
  codegen::SourceMapping,
  parser::{ParseConfig, ParserImpl},
  test::{read_file, snapshot_name},
};
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_codegen::Codegen;
use oxc_parser::ParseOptions;

pub fn format_program_codegen(program: &Program) -> String {
  Codegen::new().build(program).code
}

use oxc_ast_visit::VisitMut;
use oxc_span::{SPAN, Span};

struct SpanMapper {
  mappings: Vec<SourceMapping>,
}

impl VisitMut<'_> for SpanMapper {
  fn visit_span(&mut self, span: &mut Span) {
    *span = self
      .mappings
      .iter()
      .find(|mapping| mapping.codegen_span == *span)
      .map_or(SPAN, |mapping| mapping.original_span);
  }
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
  let mut reparsed = oxc_parser::Parser::new(&allocator, &codegen, ret.source_type)
    .with_options(ParseOptions::default())
    .parse();
  assert!(
    reparsed.errors.is_empty(),
    "Invalid codegen syntax in {file_path}: {:#?}",
    reparsed.errors,
  );

  let mut span_mapper = SpanMapper { mappings: ret.mappings };
  span_mapper.visit_program(&mut reparsed.program);

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
  program_codegen_eq(&ret.program, reparsed_program, file_path);
}

fn program_codegen_eq(left: &Program, right: &Program, file_path: &str) {
  use pretty_assertions::assert_eq;

  assert_eq!(
    format!("{:#?}", left.hashbang),
    format!("{:#?}", right.hashbang),
    "Hashbang differs for {file_path}",
  );
  assert_eq!(
    format!("{:#?}", left.directives),
    format!("{:#?}", right.directives),
    "Directives differs for {file_path}",
  );
  assert_eq!(
    format!("{:#?}", left.body),
    format!("{:#?}", right.body),
    "Body differs for {file_path}",
  );
}
