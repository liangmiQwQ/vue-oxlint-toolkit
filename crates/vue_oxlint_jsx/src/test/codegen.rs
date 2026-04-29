use oxc_allocator::Allocator;
use oxc_parser::ParseOptions;

use crate::{ParseConfig, parser::ParserImpl};

/// Tests for codegen
/// For downstream use
#[test]
fn validate_all_codegen_syntax() {
  use oxc_codegen::Codegen;
  use std::path::Path;

  fn visit_dir(path: &Path, results: &mut Vec<String>) {
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
          ParseConfig::default(),
        )
        .parse();
        if ret.fatal {
          continue;
        }
        let js = Codegen::new().build(&ret.program);
        let codegen = js.code;
        let new_allocator = Allocator::default();
        let source_type = ret.program.source_type;
        let reparsed = oxc_parser::Parser::new(&new_allocator, &codegen, source_type)
          .with_options(ParseOptions::default())
          .parse();
        if !reparsed.errors.is_empty() {
          results.push(file_path.to_string());
        }
      }
    }
  }

  let mut invalid = Vec::new();
  visit_dir(Path::new("fixtures"), &mut invalid);

  if !invalid.is_empty() {
    println!("Invalid codegen syntax in:");
    for file in &invalid {
      let snap_name = file.replace(['/', '.'], "_");
      println!("  {}  (src/parser/snapshots/ast/{}.snap)", file, snap_name);
    }
  }

  assert!(invalid.is_empty(), "Invalid codegen syntax in: {:?}", invalid);
}
