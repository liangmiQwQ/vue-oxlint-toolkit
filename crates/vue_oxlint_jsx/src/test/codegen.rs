use oxc_allocator::Allocator;
use oxc_parser::ParseOptions;

use crate::{ParseConfig, parser::ParserImpl};

/// Tests for codegen
/// For downstream use
#[test]
fn validate_all_codegen_syntax() {
  use oxc_codegen::Codegen;
  use std::path::Path;

  fn visit_dir(path: &Path, results: &mut Vec<(String, Vec<String>)>) {
    for entry in std::fs::read_dir(path).unwrap() {
      let entry = entry.unwrap();
      let path = entry.path();
      if path.is_dir() {
        visit_dir(&path, results);
      } else if path.extension().and_then(|s| s.to_str()) == Some("vue") {
        let file_path =
          path.strip_prefix("fixtures").unwrap().to_str().unwrap().trim_start_matches('/');
        let source_text = std::fs::read_to_string(&path).unwrap();
        let allocator = Allocator::default();
        let ret = ParserImpl::new(
          &allocator,
          &source_text,
          ParseOptions::default(),
          ParseConfig { codegen: true },
        )
        .parse();
        if ret.fatal {
          continue;
        }
        let js = Codegen::new().build(&ret.program);
        let codegen = js.code;

        // Store codegen as snapshot
        let snap_name = file_path.replace(['/', '.'], "_");
        let mut settings = insta::Settings::clone_current();
        settings.set_snapshot_path("snapshots/codegen");
        settings.set_prepend_module_to_snapshot(false);
        settings.bind(|| {
          insta::assert_snapshot!(snap_name, &codegen);
        });

        let new_allocator = Allocator::default();
        let source_type = ret.program.source_type;
        let reparsed = oxc_parser::Parser::new(&new_allocator, &codegen, source_type)
          .with_options(ParseOptions::default())
          .parse();
        if !reparsed.errors.is_empty() {
          results.push((
            file_path.to_string(),
            reparsed.errors.iter().map(ToString::to_string).collect(),
          ));
        }
      }
    }
  }

  let mut invalid = Vec::new();
  visit_dir(Path::new("fixtures"), &mut invalid);

  if !invalid.is_empty() {
    println!("Invalid codegen syntax in:");
    for (file_path, errors) in &invalid {
      let snap_name = file_path.replace(['/', '.'], "_");
      println!("  {file_path}  (src/snapshots/codegen/{snap_name}.snap)");
      for error in errors {
        println!("{error}");
      }
    }
  }

  let invalid_files = invalid.iter().map(|(file_path, _)| file_path).collect::<Vec<_>>();
  assert!(invalid.is_empty(), "Invalid codegen syntax in: {invalid_files:?}");
}
