pub use crate::parser::ParserImplReturn;
use crate::parser::{ParseConfig, ParserImpl};
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_ast_visit::Visit;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::{GetSpan, Span};
use std::{fmt::Write, path::Path};

mod codegen;

pub use codegen::format_program_codegen;
pub use codegen::run_codegen_test;

#[macro_export]
macro_rules! test_module_record {
  ($file_path:expr) => {{
    $crate::test::run_test($file_path, "module_record", |ret| {
      format!("Module Record: {:#?}", ret.module_record)
    });
  }};
}

const FIXTURES_DIR: &str = "../../fixtures";

#[derive(Clone, Copy)]
enum FixtureKind {
  Pass,
  Error,
  Panic,
}

impl FixtureKind {
  const fn directory(self) -> &'static str {
    match self {
      Self::Pass => "pass",
      Self::Error => "error",
      Self::Panic => "panic",
    }
  }

  const fn should_errors(self) -> bool {
    match self {
      Self::Pass => false,
      Self::Error | Self::Panic => true,
    }
  }

  const fn allow_panic(self) -> bool {
    match self {
      Self::Pass | Self::Error => false,
      Self::Panic => true,
    }
  }
}

struct Fixture {
  path: String,
  kind: FixtureKind,
}

pub struct TestResult<'a> {
  pub program: &'a Program<'a>,
  pub errors: &'a Vec<OxcDiagnostic>,
  pub codegen: String,
  pub spans: String,
}

pub fn run_fixture_tests() {
  for fixture in collect_fixtures() {
    run_ast_test(&fixture.path, fixture.kind.should_errors(), fixture.kind.allow_panic());

    if matches!(fixture.kind, FixtureKind::Pass) {
      run_codegen_test(&fixture.path);
    }
  }
}

pub fn run_ast_test(file_path: &str, should_errors: bool, allow_panic: bool) {
  run_test(file_path, "ast", |ret| {
    let codegen = format_program_codegen(&ret.program);
    let source_text = read_file(file_path);
    let node_locations = format_node_locations(&ret.program, &source_text);
    assert_eq!(
      !ret.errors.is_empty(),
      should_errors,
      "Error expectation mismatch for {file_path}. Expected has_errors: {should_errors}, but got {}",
      ret.errors.len()
    );
    assert_eq!(
      ret.fatal, allow_panic,
      "Fatal error expectation mismatch for {file_path}. Expected fatal: {allow_panic}, but got fatal: {}",
      ret.fatal
    );

    let result =
      TestResult { program: &ret.program, errors: &ret.errors, codegen, spans: node_locations };
    format!("{result:#?}")
  });
}

impl std::fmt::Debug for TestResult<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("=============== Program ===============\n")?;
    write!(f, "{:#?}", self.program)?;
    f.write_str("\n\n===============  Error  ===============\n")?;
    write!(f, "{:#?}", self.errors)?;
    f.write_str("\n\n=============== Codegen ===============\n")?;
    f.write_str(&self.codegen)?;
    f.write_str("\n\n===============  Spans  ===============\n")?;
    f.write_str(&self.spans)?;
    Ok(())
  }
}

pub fn run_test<F>(file_path: &str, folder: &str, f: F)
where
  F: for<'a> FnOnce(&ParserImplReturn<'a>) -> String,
{
  let allocator = Allocator::default();
  let source_text = read_file(file_path);

  let ret =
    ParserImpl::new(&allocator, &source_text, ParseOptions::default(), ParseConfig::default())
      .parse();

  let result = f(&ret);

  let snapshot_name = snapshot_name(file_path);
  let mut settings = insta::Settings::clone_current();
  settings.set_snapshot_path(format!("snapshots/{folder}"));
  settings.set_prepend_module_to_snapshot(false);
  settings.bind(|| {
    insta::assert_snapshot!(snapshot_name, result);
  });
}

pub fn read_file(file_path: &str) -> String {
  std::fs::read_to_string(Path::new(FIXTURES_DIR).join(file_path))
    .expect("Failed to read test file")
}

pub fn snapshot_name(file_path: &str) -> String {
  file_path.replace(['/', '\\', '.'], "_")
}

fn collect_fixtures() -> Vec<Fixture> {
  let mut fixtures = Vec::new();
  for kind in [FixtureKind::Pass, FixtureKind::Error, FixtureKind::Panic] {
    collect_fixture_kind(kind, &mut fixtures);
  }
  fixtures.sort_by(|a, b| a.path.cmp(&b.path));
  fixtures
}

fn collect_fixture_kind(kind: FixtureKind, fixtures: &mut Vec<Fixture>) {
  let root = Path::new(FIXTURES_DIR).join(kind.directory());
  collect_fixture_files(kind, &root, fixtures);
}

fn collect_fixture_files(kind: FixtureKind, directory: &Path, fixtures: &mut Vec<Fixture>) {
  let entries = std::fs::read_dir(directory).expect("Failed to read fixture directory");

  for entry in entries {
    let path = entry.expect("Failed to read fixture directory entry").path();
    if path.is_dir() {
      collect_fixture_files(kind, &path, fixtures);
      continue;
    }

    if path.extension().is_none_or(|extension| extension != "vue") {
      continue;
    }

    let relative_path =
      path.strip_prefix(FIXTURES_DIR).expect("Failed to strip fixture directory prefix");
    fixtures.push(Fixture { path: relative_path.to_string_lossy().replace('\\', "/"), kind });
  }
}

fn format_string_slice(s: &str) -> String {
  if s.len() <= 80 {
    s.to_string()
  } else {
    let chars: Vec<char> = s.chars().collect();
    let start: String = chars.iter().take(40).collect();
    let end: String = chars.iter().rev().take(40).rev().collect();
    format!("{start}..[OMIT]..{end}")
  }
}

struct NodeLocationCollector<'a> {
  source_text: &'a str,
  locations: Vec<(Span, String, String)>,
}

impl<'a> NodeLocationCollector<'a> {
  fn new(source_text: &'a str) -> Self {
    Self { source_text, locations: Vec::new() }
  }

  fn add_span(&mut self, span: Span, kind: String) {
    let start = span.start as usize;
    let end = span.end as usize;
    if !span.is_empty() {
      let slice = &self.source_text[start..end];
      let formatted_slice = format_string_slice(slice);
      let kind = match memchr::memchr(b'(', kind.as_bytes()) {
        Some(index) => kind[..index].to_owned(),
        None => kind,
      };
      self.locations.push((span, formatted_slice, kind));
    }
  }
}

impl<'a> Visit<'a> for NodeLocationCollector<'a> {
  fn enter_node(&mut self, kind: oxc_ast::AstKind<'a>) {
    self.add_span(kind.span(), format!("{kind:?}"));
  }
}

pub fn format_node_locations(program: &Program, source_text: &str) -> String {
  let mut collector = NodeLocationCollector::new(source_text);
  collector.visit_program(program);

  let mut result = String::new();
  for (span, slice, kind) in collector.locations {
    let start = span.start;
    let end = span.end;
    let _ = write!(result, "Slice: {slice:?}; \nSpan: ({start}, {end}); \nType: {kind}; \n\n");
  }
  result
}
