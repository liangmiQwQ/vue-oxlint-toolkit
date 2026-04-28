//! Fixture-driven parser tests.
//!
//! Each directory under `tests/fixtures/ast/` provides a `source.vue` and a
//! `tree.json` (the upstream `vue-eslint-parser` simplified tree shape: a list
//! of `{type, text, children}` objects rooted at each `<template>` block).
//!
//! We walk every fixture, project our V* AST into the same shape, and compare.
//! Mismatches are collected and reported together at the end, like a simpler
//! `insta`. Unimplemented features surface as failing fixtures; that's
//! intentional for now.

use serde_json::{Value, json};
use std::path::PathBuf;

#[test]
fn ast_fixtures() {
  let dir = fixtures_dir();
  let mut names: Vec<String> = std::fs::read_dir(&dir)
    .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
    .filter_map(Result::ok)
    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
    .filter_map(|e| e.file_name().into_string().ok())
    .collect();
  names.sort();

  let mut passed = 0usize;
  let mut failures: Vec<String> = Vec::new();

  for name in &names {
    match run_one(name) {
      Ok(()) => passed += 1,
      Err(msg) => failures.push(format!("[FAIL] {name}\n{msg}")),
    }
  }

  let total = names.len();
  let failed = failures.len();
  eprintln!("ast fixtures: {passed} passed, {failed} failed of {total}");

  if !failures.is_empty() {
    let only = std::env::var("AST_FIXTURE_VERBOSE").is_ok();
    let body = if only {
      failures.join("\n\n")
    } else {
      failures
        .iter()
        .map(|f| f.lines().next().unwrap_or("").to_string())
        .collect::<Vec<_>>()
        .join("\n")
    };
    panic!("{failed}/{total} ast fixtures failed (set AST_FIXTURE_VERBOSE=1 for diffs):\n{body}");
  }
}

fn run_one(name: &str) -> Result<(), String> {
  let dir = fixtures_dir().join(name);
  let source =
    std::fs::read_to_string(dir.join("source.vue")).map_err(|e| format!("read source.vue: {e}"))?;
  let expected: Value = serde_json::from_str(
    &std::fs::read_to_string(dir.join("tree.json")).map_err(|e| format!("read tree.json: {e}"))?,
  )
  .map_err(|e| format!("parse tree.json: {e}"))?;

  let parsed = parse_fixture(&source)?;
  let actual = build_template_tree(&parsed, &source);

  if actual == expected {
    return Ok(());
  }
  let exp = serde_json::to_string_pretty(&expected).unwrap_or_default();
  let act = serde_json::to_string_pretty(&actual).unwrap_or_default();
  Err(format!("--- expected ---\n{exp}\n--- actual ---\n{act}"))
}

fn fixtures_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ast")
}

fn parse_fixture(source: &str) -> Result<Value, String> {
  let json_str =
    vue_oxlint_parser::parse_to_json(source, &vue_oxlint_parser::ParseOptions::default())
      .map_err(|d| d.to_string())?;
  serde_json::from_str(&json_str).map_err(|e| e.to_string())
}

fn build_template_tree(parsed: &Value, src: &str) -> Value {
  let mut out = Vec::new();
  let Some(children) =
    parsed.get("document").and_then(|d| d.get("children")).and_then(Value::as_array)
  else {
    return Value::Array(out);
  };
  for child in children {
    let Some(el) = unwrap_element_child(child) else { continue };
    if el.get("name").and_then(Value::as_str) == Some("template") {
      out.push(walk(el, src));
    }
  }
  Value::Array(out)
}

/// Top-level `VRootChild` is `untagged`, so a `VElement` appears bare; an
/// element-level `VElementChild` is wrapped as `{"VElement": {...}}` /
/// `{"VText": {...}}` / `{"VExpressionContainer": {...}}`.
fn unwrap_element_child(node: &Value) -> Option<&Value> {
  if node.get("type").is_some() {
    return Some(node);
  }
  let obj = node.as_object()?;
  obj.values().next()
}

fn walk(node: &Value, src: &str) -> Value {
  let typ = node.get("type").and_then(Value::as_str).unwrap_or("?");
  let text = slice(src, node.get("range")).unwrap_or_default().to_string();
  let children = collect_children(typ, node, src);
  json!({ "type": typ, "text": text, "children": children })
}

fn slice<'a>(src: &'a str, range: Option<&Value>) -> Option<&'a str> {
  let r = range?;
  let start = r.get("start").and_then(Value::as_u64)? as usize;
  let end = r.get("end").and_then(Value::as_u64)? as usize;
  src.get(start..end)
}

fn collect_children(typ: &str, node: &Value, src: &str) -> Vec<Value> {
  match typ {
    "VElement" => {
      let mut v = Vec::new();
      if let Some(st) = node.get("start_tag") {
        v.push(walk(st, src));
      }
      if let Some(arr) = node.get("children").and_then(Value::as_array) {
        for c in arr {
          if let Some(inner) = unwrap_element_child(c) {
            v.push(walk(inner, src));
          }
        }
      }
      if let Some(et) = node.get("end_tag")
        && !et.is_null()
      {
        v.push(walk(et, src));
      }
      v
    }
    "VStartTag" => node
      .get("attributes")
      .and_then(Value::as_array)
      .map(|a| a.iter().map(|x| walk(x, src)).collect())
      .unwrap_or_default(),
    "VAttribute" => {
      let mut v = Vec::new();
      if let Some(k) = node.get("key") {
        v.push(walk(k, src));
      }
      if let Some(val) = node.get("value")
        && !val.is_null()
      {
        v.push(walk(val, src));
      }
      v
    }
    _ => Vec::new(),
  }
}
