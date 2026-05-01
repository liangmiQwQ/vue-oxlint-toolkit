use crate::{
  VueJsxCodegen,
  codegen::SourceMapping,
  parser::{ParseConfig, ParserImpl},
  test::{read_file, snapshot_name},
};
use oxc_allocator::Allocator;
use oxc_ast::{AstKind, ast::Program};
use oxc_codegen::Codegen;
use oxc_parser::ParseOptions;

pub fn format_program_codegen(program: &Program) -> String {
  Codegen::new().build(program).code
}

use oxc_ast_visit::{Visit, VisitMut};
use oxc_span::{ContentEq, GetSpan, SPAN, Span};

struct SpanMapper {
  mappings: Vec<SourceMapping>,
}

impl VisitMut<'_> for SpanMapper {
  fn visit_span(&mut self, span: &mut Span) {
    // Translate each codegen-output span back to its original SFC span.
    // VisitMut may visit the same span field more than once, so keep already
    // translated spans intact. Synthetic generated nodes with no source mapping
    // are zeroed so they compare equal to SPAN placeholders in the origin AST.
    if let Some(mapping) = self
      .mappings
      .iter()
      .filter(|mapping| mapping.codegen_span == *span)
      .min_by_key(|mapping| mapping.original_span.end - mapping.original_span.start)
    {
      *span = mapping.original_span;
    } else if !self.mappings.iter().any(|mapping| mapping.original_span == *span) {
      *span = self
        .mappings
        .iter()
        .filter(|mapping| {
          mapping.codegen_span.start <= span.start
            && mapping.codegen_span.end <= span.end
            && span.start - mapping.codegen_span.start <= 1
            && span.end - mapping.codegen_span.end <= 1
        })
        .min_by_key(|mapping| mapping.codegen_span.end - mapping.codegen_span.start)
        .map_or(SPAN, |mapping| mapping.original_span);
    }
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

fn program_codegen_eq(origin: &Program, reparsed: &Program, file_path: &str) {
  assert!(origin.hashbang.content_eq(&reparsed.hashbang), "Hashbang differs for {file_path}");
  assert!(origin.directives.content_eq(&reparsed.directives), "Directives differs for {file_path}");
  assert!(origin.body.content_eq(&reparsed.body), "Body differs for {file_path}");

  let origin_spans = collect_spans(origin);
  let reparsed_spans = collect_spans(reparsed);
  origin_spans.into_iter().zip(reparsed_spans).for_each(|(origin_span, reparsed_span)| {
    assert_eq!(origin_span, reparsed_span, "[MAPPING] Span differ for {file_path}");
  });
}

fn collect_spans(program: &Program) -> Vec<(String, Span)> {
  let mut collector = SpanCollector { spans: Vec::new() };
  collector.visit_program(program);
  collector.spans
}

struct SpanCollector {
  spans: Vec<(String, Span)>,
}

impl<'a> Visit<'a> for SpanCollector {
  fn enter_node(&mut self, kind: oxc_ast::AstKind<'a>) {
    // These nodes are generated with wrapper punctuation, deferred semicolons, or
    // synthetic Vue JSX scaffolding that does not have a stable one-to-one source
    // range. Token nodes inside them are still collected and checked.
    if matches!(
      kind,
      AstKind::ExpressionStatement(_)
        | AstKind::JSXOpeningFragment(_)
        | AstKind::JSXClosingFragment(_)
        | AstKind::JSXIdentifier(_)
        | AstKind::FormalParameters(_)
        | AstKind::FormalParameter(_)
        | AstKind::ObjectPattern(_)
        | AstKind::Function(_)
        | AstKind::ImportSpecifier(_)
        | AstKind::TSTypeAnnotation(_)
        | AstKind::TSLiteralType(_)
    ) {
      return;
    }

    let kind_str = format!("{kind:?}");
    let kind_name = match memchr::memchr(b'(', kind_str.as_bytes()) {
      Some(index) => kind_str[..index].to_owned(),
      None => kind_str,
    };
    self.spans.push((kind_name, kind.span()));
  }
}
