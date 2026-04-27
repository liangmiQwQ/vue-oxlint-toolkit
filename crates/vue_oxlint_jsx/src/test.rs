use crate::parser::ParserImpl;
pub use crate::parser::ParserImplReturn;
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_ast_visit::Visit;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::{GetSpan, Span};
use std::fmt::Write;

#[macro_export]
macro_rules! test_ast {
  ($file_path:expr) => {
    test_ast!($file_path, false, false);
  };
  ($file_path:expr, $should_errors:expr, $should_panic:expr) => {{
    $crate::test::run_test($file_path, "ast", |ret| {
      use oxc_codegen::Codegen;
      let js = Codegen::new().build(&ret.program);
      let source_text = $crate::test::read_file($file_path);
      let node_locations = $crate::test::format_node_locations(&ret.program, &source_text);
      assert_eq!(
        !ret.errors.is_empty(),
        $should_errors,
        "Error expectation mismatch for {}. Expected has_errors: {}, but got {}",
        $file_path,
        $should_errors,
        ret.errors.len()
      );
      assert_eq!(
        ret.fatal, $should_panic,
        "Fatal error expectation mismatch for {}. Expected fatal: {}, but got fatal: {}",
        $file_path, $should_panic, ret.fatal
      );

      let result = $crate::test::TestResult {
        program: &ret.program,
        errors: &ret.errors,
        codegen: js.code,
        spans: node_locations,
      };
      format!("{result:#?}")
    });
  }};
}

#[macro_export]
macro_rules! test_module_record {
  ($file_path:expr) => {{
    $crate::test::run_test($file_path, "module_record", |ret| {
      format!("Module Record: {:#?}", ret.module_record)
    });
  }};
}

pub struct TestResult<'a> {
  pub program: &'a Program<'a>,
  pub errors: &'a Vec<OxcDiagnostic>,
  pub codegen: String,
  pub spans: String,
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

  let ret = ParserImpl::new(&allocator, &source_text, ParseOptions::default()).parse();

  let result = f(&ret);

  let snapshot_name = file_path.replace(['/', '.'], "_");
  let mut settings = insta::Settings::clone_current();
  settings.set_snapshot_path(format!("parser/snapshots/{folder}"));
  settings.set_prepend_module_to_snapshot(false);
  settings.bind(|| {
    insta::assert_snapshot!(snapshot_name, result);
  });
}

pub fn read_file(file_path: &str) -> String {
  std::fs::read_to_string(format!("fixtures/{file_path}")).expect("Failed to read test file")
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
