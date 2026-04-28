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
