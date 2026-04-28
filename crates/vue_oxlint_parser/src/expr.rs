//! On-demand expression parsing for `VExpressionContainer`.
//!
//! When the AST is serialised, each `VExpressionContainer` runs the inner
//! expression text through `oxc_parser::parse_expression`, serialises the
//! resulting node via `oxc_estree`, and shifts every `start`/`end` offset
//! by the container's source position so the embedded JSON ranges align with
//! the original SFC source.

use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_estree::{CompactTSSerializer, ESTree};
use oxc_parser::Parser as JsParser;
use oxc_span::{GetSpan, SourceType};
use serde_json::{Value, value::RawValue};

use crate::ast::Span;

/// Parse the entire `text` as a JS program (statements) and return the
/// `program.body` array as `ESTree` JSON, with all `start`/`end` shifted by
/// `base` and `range` mirrors added. Returns `None` on parse failure.
fn parse_program_body(text: &str, base: u32) -> Option<Value> {
  if text.trim().is_empty() {
    return None;
  }
  let alloc = Allocator::default();
  let st = SourceType::default().with_module(true);
  let ret = JsParser::new(&alloc, text, st).parse();
  if !ret.errors.is_empty() {
    return None;
  }
  let mut ser = CompactTSSerializer::new(true);
  ret.program.serialize(&mut ser);
  let body = ser.into_string();
  let mut value: Value = serde_json::from_str(&body).ok()?;
  shift_ranges(&mut value, base);
  add_range_objects(&mut value);
  // `Program.body` is the array we want.
  let body_val = value.as_object_mut()?.remove("body")?;
  Some(body_val)
}

fn raw_value_from(v: &Value) -> Option<Box<RawValue>> {
  RawValue::from_string(serde_json::to_string(v).ok()?).ok()
}

/// Build a synthetic Vue wrapper node `{type, start, end, range, ...extra}`
/// from a span and additional fields. The extra map is consumed.
fn wrap_node(type_name: &str, span: Span, mut extra: serde_json::Map<String, Value>) -> Value {
  extra.insert("type".into(), Value::String(type_name.into()));
  extra.insert("start".into(), Value::from(span.start));
  extra.insert("end".into(), Value::from(span.end));
  extra.insert("range".into(), serde_json::json!({ "start": span.start, "end": span.end }));
  Value::Object(extra)
}

/// Wrap a `v-on` directive value: parse the inner text as a sequence of
/// statements (`a=b`, `f(); g()`) and embed them as `body` of a synthetic
/// `VOnExpression`. Range covers the directive's inner text.
#[must_use]
pub fn parse_v_on_to_raw(text: &str, span: Span) -> Option<Box<RawValue>> {
  let Some(body) = parse_program_body(text, span.start) else {
    // Some valid handler expressions are not valid statement lists
    // (e.g. `function() {}` and `async function() {}`), so fall back to
    // expression parsing and expose the parsed expression directly.
    return parse_expression_to_raw(text, span.start);
  };
  let arr = body.as_array()?;
  // Special case: a single ExpressionStatement whose expression is a
  // "simple path" (Identifier / MemberExpression) or a function-like value
  // (FunctionExpression / ArrowFunctionExpression) is exposed as the bare
  // expression with no `VOnExpression` wrapper — matches upstream.
  if arr.len() == 1 {
    let stmt = &arr[0];
    if stmt.get("type").and_then(Value::as_str) == Some("ExpressionStatement")
      && let Some(inner) = stmt.get("expression")
      && is_simple_v_on_expression(inner)
    {
      return raw_value_from(inner);
    }
  }
  let mut map = serde_json::Map::new();
  map.insert("body".into(), body);
  raw_value_from(&wrap_node("VOnExpression", span, map))
}

fn is_simple_v_on_expression(v: &Value) -> bool {
  let Some(t) = v.get("type").and_then(Value::as_str) else {
    return false;
  };
  matches!(
    t,
    "Identifier"
      | "ThisExpression"
      | "MemberExpression"
      | "ChainExpression"
      | "FunctionExpression"
      | "ArrowFunctionExpression"
  )
}

/// Wrap a `v-slot` (or `slot-scope`) directive value as
/// `VSlotScopeExpression { params: [<binding pattern>] }`.
///
/// We parse a synthetic arrow function `(<text>) => 0` and lift its single
/// parameter — this lets the JS parser accept destructuring, defaults, and
/// rest patterns just like a real function parameter list.
#[must_use]
pub fn parse_v_slot_to_raw(text: &str, span: Span) -> Option<Box<RawValue>> {
  let trimmed = text.trim();
  if trimmed.is_empty() {
    return None;
  }
  // Find the leading-whitespace offset so we can shift parser-output ranges
  // back to the original source position of the inner text.
  let leading_ws = text.len() - text.trim_start().len();
  let inner_start = span.start + leading_ws as u32;
  // Build wrapper `(TEXT) => 0`. The argument starts at offset 1.
  let wrapped = format!("({trimmed}) => 0");
  let alloc = Allocator::default();
  let st = SourceType::default().with_module(true);
  let ret = JsParser::new(&alloc, &wrapped, st).parse();
  if !ret.errors.is_empty() {
    return None;
  }
  let mut ser = CompactTSSerializer::new(true);
  ret.program.serialize(&mut ser);
  let body = ser.into_string();
  let mut value: Value = serde_json::from_str(&body).ok()?;
  // Shift everything by inner_start - 1 (account for the leading `(`).
  let shift = inner_start.saturating_sub(1);
  shift_ranges(&mut value, shift);
  add_range_objects(&mut value);
  // Walk: program.body[0].expression.params
  let body_arr = value.as_object_mut()?.remove("body")?;
  let mut body_vec = body_arr.as_array().cloned().unwrap_or_default();
  if body_vec.is_empty() {
    return None;
  }
  let stmt = body_vec.remove(0);
  let expr = stmt.get("expression").cloned()?;
  let params = expr.get("params").cloned().unwrap_or(Value::Array(vec![]));
  let mut map = serde_json::Map::new();
  map.insert("params".into(), params);
  raw_value_from(&wrap_node("VSlotScopeExpression", span, map))
}

/// Wrap a `v-for` directive value as
/// `VForExpression { left: [...], right: <expr> }`.
///
/// Splits on the *unparenthesised* keyword ` in ` or ` of `, parses the
/// left side as the parameter list of a synthetic arrow function and the
/// right side as a single expression.
#[must_use]
pub fn parse_v_for_to_raw(text: &str, span: Span) -> Option<Box<RawValue>> {
  let (left_raw, right_raw, sep_at) = split_v_for(text)?;
  let left_local = trim_offset(text, 0, sep_at);
  let right_local = trim_offset(text, sep_at, text.len());

  let left_trimmed = &text[left_local.0..left_local.1];
  let right_trimmed = &text[right_local.0..right_local.1];
  let left_base = span.start + left_local.0 as u32;
  let right_base = span.start + right_local.0 as u32;

  // Left side: parse `(LEFT) => 0` and lift params (supports `(a,b,c)` and
  // bare `a` alike).
  let left_value = if left_trimmed.is_empty() {
    Value::Array(vec![])
  } else {
    parse_arrow_params(left_trimmed, left_base)?
  };

  // Right side: a single expression.
  let right_value: Value = {
    let raw = parse_expression_to_raw(right_trimmed, right_base)?;
    serde_json::from_str(raw.get()).ok()?
  };
  let _ = (left_raw, right_raw);

  let mut map = serde_json::Map::new();
  map.insert("left".into(), left_value);
  map.insert("right".into(), right_value);
  raw_value_from(&wrap_node("VForExpression", span, map))
}

/// Parse `text` as the parameter list of a synthetic arrow function and
/// return the resulting `params` array as JSON. `base` is the source offset
/// the inner text begins at.
fn parse_arrow_params(text: &str, base: u32) -> Option<Value> {
  let wrapped = format!("({text}) => 0");
  let alloc = Allocator::default();
  let st = SourceType::default().with_module(true);
  let ret = JsParser::new(&alloc, &wrapped, st).parse();
  if !ret.errors.is_empty() {
    return None;
  }
  let mut ser = CompactTSSerializer::new(true);
  ret.program.serialize(&mut ser);
  let mut value: Value = serde_json::from_str(&ser.into_string()).ok()?;
  let shift = base.saturating_sub(1);
  shift_ranges(&mut value, shift);
  add_range_objects(&mut value);
  let body_arr = value.as_object_mut()?.remove("body")?;
  let mut body_vec = body_arr.as_array().cloned().unwrap_or_default();
  if body_vec.is_empty() {
    return None;
  }
  let stmt = body_vec.remove(0);
  let expr = stmt.get("expression").cloned()?;
  Some(expr.get("params").cloned().unwrap_or(Value::Array(vec![])))
}

/// Find the byte index in `text` where `' in '` or `' of '` starts. Skips
/// occurrences inside parenthesised groups so `(a, b) in xs.in` works.
fn split_v_for(text: &str) -> Option<(&str, &str, usize)> {
  let bytes = text.as_bytes();
  let mut depth = 0i32;
  let mut i = 0usize;
  while i < bytes.len() {
    match bytes[i] {
      b'(' | b'[' | b'{' => depth += 1,
      b')' | b']' | b'}' => depth -= 1,
      _ if depth == 0 && bytes[i].is_ascii_whitespace() => {
        let after = &bytes[i + 1..];
        if after.starts_with(b"in ")
          || after.starts_with(b"in\t")
          || after.starts_with(b"in\n")
          || after.starts_with(b"of ")
          || after.starts_with(b"of\t")
          || after.starts_with(b"of\n")
        {
          return Some((&text[..i], &text[i + 4..], i));
        }
      }
      _ => {}
    }
    i += 1;
  }
  None
}

/// Compute the trimmed sub-range `(lo, hi)` of `&text[from..to]` such that
/// neither end is whitespace.
fn trim_offset(text: &str, from: usize, to: usize) -> (usize, usize) {
  let bytes = text.as_bytes();
  let mut lo = from;
  let mut hi = to;
  while lo < hi && bytes[lo].is_ascii_whitespace() {
    lo += 1;
  }
  while hi > lo && bytes[hi - 1].is_ascii_whitespace() {
    hi -= 1;
  }
  (lo, hi)
}

/// Build an `Identifier`-shaped JSON node directly.
///
/// This bypasses `oxc_parser` for `v-bind` same-name shorthand where the
/// argument text may not be a syntactically valid JS identifier on its own
/// (e.g. `aria-label`) but vue-eslint-parser still synthesizes one.
#[must_use]
pub fn synthetic_identifier_raw(name: &str, span: Span) -> Option<Box<RawValue>> {
  if name.is_empty() {
    return None;
  }
  let body = serde_json::json!({
    "type": "Identifier",
    "name": name,
    "start": span.start,
    "end": span.end,
    "range": { "start": span.start, "end": span.end },
  });
  RawValue::from_string(serde_json::to_string(&body).ok()?).ok()
}

/// Parse `text` as a JS expression and return its `ESTree` JSON.
///
/// Every `start`/`end` is shifted by `base`. Returns `None` if `text` is not
/// a single valid expression that consumes the entire input.
#[must_use]
pub fn parse_expression_to_raw(text: &str, base: u32) -> Option<Box<RawValue>> {
  if text.trim().is_empty() {
    return None;
  }
  let alloc = Allocator::default();
  let st = SourceType::default().with_module(true);
  let expr = JsParser::new(&alloc, text, st).parse_expression().ok()?;
  let trimmed_end = text.trim_end().len() as u32;
  if expression_end(&expr) < trimmed_end {
    return None;
  }
  let mut ser = CompactTSSerializer::new(true);
  expr.serialize(&mut ser);
  let body = ser.into_string();
  let mut value: Value = serde_json::from_str(&body).ok()?;
  shift_ranges(&mut value, base);
  add_range_objects(&mut value);
  let s = serde_json::to_string(&value).ok()?;
  RawValue::from_string(s).ok()
}

fn expression_end(e: &Expression<'_>) -> u32 {
  e.span().end
}

fn shift_ranges(v: &mut Value, base: u32) {
  match v {
    Value::Object(map) => {
      let keys: Vec<String> = map.keys().cloned().collect();
      for k in keys {
        if k == "start" || k == "end" {
          if let Some(n) = map.get(&k).and_then(Value::as_u64) {
            map.insert(k, Value::from(n + u64::from(base)));
          }
        } else if let Some(child) = map.get_mut(&k) {
          shift_ranges(child, base);
        }
      }
    }
    Value::Array(arr) => {
      for x in arr {
        shift_ranges(x, base);
      }
    }
    _ => {}
  }
}

/// Mirror each node's `start`/`end` into a nested `range: {start, end}` so
/// downstream walkers that look at `range` see the same offsets `oxc_estree`
/// publishes flat on the node.
fn add_range_objects(v: &mut Value) {
  match v {
    Value::Object(map) => {
      let start = map.get("start").and_then(Value::as_u64);
      let end = map.get("end").and_then(Value::as_u64);
      if let (Some(s), Some(e)) = (start, end) {
        let mut range = serde_json::Map::new();
        range.insert("start".into(), Value::from(s));
        range.insert("end".into(), Value::from(e));
        map.insert("range".into(), Value::Object(range));
      }
      let keys: Vec<String> = map.keys().cloned().collect();
      for k in keys {
        if k == "range" {
          continue;
        }
        if let Some(child) = map.get_mut(&k) {
          add_range_objects(child);
        }
      }
    }
    Value::Array(arr) => {
      for x in arr {
        add_range_objects(x);
      }
    }
    _ => {}
  }
}
