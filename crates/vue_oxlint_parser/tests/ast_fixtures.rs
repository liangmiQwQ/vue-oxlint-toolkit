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
use std::fmt::Write;
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

  // Compare observed failures against the tracked allowlist. The test is
  // green when failures match exactly: regressions (newly-failing fixtures)
  // and surprises (allowlisted fixtures that now pass) both flag the run.
  let allowlist: std::collections::BTreeSet<String> =
    std::fs::read_to_string(fixtures_dir().parent().unwrap().join("expected-failures.txt"))
      .unwrap_or_default()
      .lines()
      .map(str::trim)
      .filter(|l| !l.is_empty() && !l.starts_with('#'))
      .map(String::from)
      .collect();
  let observed: std::collections::BTreeSet<String> = failures
    .iter()
    .map(|f| f.lines().next().unwrap_or("").trim_start_matches("[FAIL] ").to_string())
    .collect();

  let regressions: Vec<&String> = observed.difference(&allowlist).collect();
  let unexpected_passes: Vec<&String> = allowlist.difference(&observed).collect();

  if regressions.is_empty() && unexpected_passes.is_empty() {
    return;
  }

  let mut msg = String::new();
  if !regressions.is_empty() {
    let _ = write!(msg, "\nRegressions ({}):\n", regressions.len());
    for r in &regressions {
      let _ = writeln!(msg, "  - {r}");
    }
  }
  if !unexpected_passes.is_empty() {
    let _ = write!(
      msg,
      "\nFixtures now pass (remove from expected-failures.txt) ({}):\n",
      unexpected_passes.len()
    );
    for r in &unexpected_passes {
      let _ = writeln!(msg, "  - {r}");
    }
  }
  if std::env::var("AST_FIXTURE_VERBOSE").is_ok() {
    msg.push_str("\n--- diffs ---\n");
    msg.push_str(&failures.join("\n\n"));
  }
  panic!("{msg}");
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
    "VDirectiveKey" => {
      let mut v = Vec::new();
      if let Some(name) = node.get("name")
        && !name.is_null()
      {
        v.push(walk(name, src));
      }
      if let Some(arg) = node.get("argument")
        && !arg.is_null()
      {
        v.push(walk(arg, src));
      }
      if let Some(mods) = node.get("modifiers").and_then(Value::as_array) {
        for m in mods {
          v.push(walk(m, src));
        }
      }
      v
    }
    "VExpressionContainer" => {
      let mut v = Vec::new();
      if let Some(expr) = node.get("expression")
        && !expr.is_null()
      {
        v.push(walk(expr, src));
      }
      v
    }
    // Default: treat as a generic ESTree JS node — recurse into every field
    // whose value is itself a node (has a `type`) or a list of nodes.
    _ => generic_js_children(node, src),
  }
}

fn generic_js_children(node: &Value, src: &str) -> Vec<Value> {
  let mut out = Vec::new();
  let Some(obj) = node.as_object() else { return out };
  for (k, val) in obj {
    if matches!(k.as_str(), "type" | "range" | "start" | "end" | "loc" | "raw" | "value" | "name") {
      continue;
    }
    visit_js_value(val, &mut out, src);
  }
  out
}

fn visit_js_value(v: &Value, out: &mut Vec<Value>, src: &str) {
  match v {
    Value::Object(map) => {
      if map.contains_key("type") && map.contains_key("start") {
        out.push(walk(v, src));
      }
    }
    Value::Array(arr) => {
      for x in arr {
        visit_js_value(x, out, src);
      }
    }
    _ => {}
  }
}
