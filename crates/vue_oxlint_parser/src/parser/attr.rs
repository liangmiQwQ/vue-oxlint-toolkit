//! Attribute construction.
//!
//! Consumes lexer-emitted attribute tokens (collected per start tag) and
//! produces `VAttribute` AST nodes. Directive keys are split here into
//! `name`, optional `argument`, and `modifiers`; `v-on` / `v-slot` /
//! `v-for` are tagged with the right `VExprKind` so the on-demand
//! expression parser knows how to wrap their values.

use oxc_allocator::{Allocator, Box as ArenaBox, Vec as ArenaVec};
use oxc_span::SourceType;
use oxc_str::Str;

use crate::ast::{
  Span, VAttribute, VAttributeKey, VAttributeValue, VDirectiveKey, VDirectiveKeyArgument,
  VExprKind, VExpressionContainer, VIdentifier, VLiteral,
};

/// One attribute as captured during start-tag lexing.
#[derive(Debug, Clone, Copy)]
pub struct AttrTok<'a> {
  pub key_span: Span,
  pub key: &'a str,
  pub value: Option<AttrValue<'a>>,
  /// Rightmost byte covered by this attribute. Equals `key_span.end`
  /// when the attribute is bare; extends over a trailing `=` when the
  /// value is missing; equals `value.outer_span.end` when present.
  pub attr_end: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct AttrValue<'a> {
  pub outer_span: Span,
  pub inner_span: Span,
  pub text: &'a str,
  pub quoted: bool,
}

pub fn build_vattributes<'a>(
  alloc: &'a Allocator,
  attrs: &[AttrTok<'a>],
  in_v_pre: bool,
  source_type: SourceType,
) -> ArenaVec<'a, VAttribute<'a>> {
  let mut out: ArenaVec<'a, VAttribute<'a>> = ArenaVec::new_in(alloc);
  for a in attrs {
    out.push(build_one(alloc, a, in_v_pre, source_type));
  }
  out
}

fn build_one<'a>(
  alloc: &'a Allocator,
  a: &AttrTok<'a>,
  in_v_pre: bool,
  source_type: SourceType,
) -> VAttribute<'a> {
  let is_v_pre_attr = a.key == "v-pre";
  let (key_node, is_directive) = classify_key(alloc, a.key, a.key_span, in_v_pre && !is_v_pre_attr);

  let attr_span = Span::new(a.key_span.start, a.attr_end);

  let value_kind: VExprKind = if let VAttributeKey::Directive(dk) = &*key_node {
    let n = dk.name.name.as_str();
    if n == "@" || n.eq_ignore_ascii_case("v-on") {
      VExprKind::VOn
    } else if n == "#" || n.eq_ignore_ascii_case("v-slot") || n.eq_ignore_ascii_case("slot-scope") {
      VExprKind::VSlot
    } else if n.eq_ignore_ascii_case("v-for") {
      VExprKind::VFor
    } else {
      VExprKind::Default
    }
  } else {
    VExprKind::Default
  };

  let synth_value: Option<(Span, &'a str)> = if a.value.is_none()
    && is_directive
    && let VAttributeKey::Directive(dk) = &*key_node
  {
    let name_str = dk.name.name;
    let is_bind = name_str.as_ref() == ":" || name_str.as_ref().eq_ignore_ascii_case("v-bind");
    if is_bind && let Some(VDirectiveKeyArgument::Identifier(id)) = &dk.argument {
      let n = id.name.as_str();
      if !n.is_empty() && is_plausible_arg_name(n) { Some((id.range, n)) } else { None }
    } else {
      None
    }
  } else {
    None
  };

  let value_node = a.value.map(|v| {
    if is_directive {
      ArenaBox::new_in(
        VAttributeValue::Expression(VExpressionContainer::new(
          v.outer_span,
          Str::from(v.text),
          v.inner_span,
          false,
          false,
          value_kind,
          source_type,
        )),
        alloc,
      )
    } else {
      ArenaBox::new_in(
        VAttributeValue::Literal(VLiteral::new(v.outer_span, Str::from(v.text))),
        alloc,
      )
    }
  });

  let value_node = value_node.or_else(|| {
    synth_value.map(|(arg_span, name)| {
      ArenaBox::new_in(
        VAttributeValue::Expression(VExpressionContainer::new(
          arg_span,
          Str::from(name),
          arg_span,
          false,
          true,
          VExprKind::Default,
          source_type,
        )),
        alloc,
      )
    })
  });

  VAttribute::new(attr_span, is_directive, key_node, value_node)
}

fn is_plausible_arg_name(s: &str) -> bool {
  let bytes = s.as_bytes();
  if bytes.is_empty() {
    return false;
  }
  if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
    return false;
  }
  bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

fn classify_key<'a>(
  alloc: &'a Allocator,
  raw: &'a str,
  span: Span,
  force_plain: bool,
) -> (ArenaBox<'a, VAttributeKey<'a>>, bool) {
  if force_plain || raw.is_empty() {
    return (plain_identifier(alloc, raw, span), false);
  }
  let bytes = raw.as_bytes();
  let name_len = match bytes[0] {
    b':' | b'@' | b'#' | b'.' => 1,
    _ if raw.starts_with("v-") => {
      let after = &raw[2..];
      let after_end = after.find([':', '.']).unwrap_or(after.len());
      2 + after_end
    }
    _ => return (plain_identifier(alloc, raw, span), false),
  };
  parse_directive_key(alloc, raw, name_len, span)
}

fn plain_identifier<'a>(
  alloc: &'a Allocator,
  raw: &'a str,
  span: Span,
) -> ArenaBox<'a, VAttributeKey<'a>> {
  ArenaBox::new_in(
    VAttributeKey::Identifier(VIdentifier::new(span, Str::from(raw), Str::from(raw))),
    alloc,
  )
}

fn parse_directive_key<'a>(
  alloc: &'a Allocator,
  raw: &'a str,
  name_len: usize,
  span: Span,
) -> (ArenaBox<'a, VAttributeKey<'a>>, bool) {
  let name_text = &raw[..name_len];
  let name_ident = VIdentifier::new(
    Span::new(span.start, span.start + name_len as u32),
    Str::from(name_text),
    Str::from(name_text),
  );

  let rest = &raw[name_len..];
  let bytes0 = raw.as_bytes()[0];
  let is_shorthand = matches!(bytes0, b':' | b'@' | b'#' | b'.');
  let is_prop_shorthand = bytes0 == b'.';

  let (arg_offset, arg_text, after_arg_idx, dynamic) =
    if !is_shorthand && let Some(after_colon) = rest.strip_prefix(':') {
      classify_arg(after_colon, name_len + 1)
    } else if is_shorthand && !rest.is_empty() && rest.as_bytes()[0] != b'.' {
      classify_arg(rest, name_len)
    } else {
      (name_len, "", name_len, false)
    };

  let argument = if arg_text.is_empty() && !dynamic {
    None
  } else if dynamic {
    let outer = Span::new(span.start + arg_offset as u32, span.start + after_arg_idx as u32);
    let inner = Span::new(outer.start + 1, outer.end - 1);
    Some(VDirectiveKeyArgument::Expression(VExpressionContainer::new(
      outer,
      Str::from(arg_text),
      inner,
      false,
      false,
      VExprKind::Default,
      SourceType::default().with_module(true),
    )))
  } else {
    Some(VDirectiveKeyArgument::Identifier(VIdentifier::new(
      Span::new(span.start + arg_offset as u32, span.start + after_arg_idx as u32),
      Str::from(arg_text),
      Str::from(arg_text),
    )))
  };

  let mut modifiers: ArenaVec<'a, VIdentifier<'a>> = ArenaVec::new_in(alloc);
  let mut cursor = after_arg_idx;
  while cursor < raw.len() && raw.as_bytes()[cursor] == b'.' {
    let mod_lo = cursor + 1;
    let rest = &raw[mod_lo..];
    let dot = rest.find('.').unwrap_or(rest.len());
    let mod_hi = mod_lo + dot;
    let text = &raw[mod_lo..mod_hi];
    modifiers.push(VIdentifier::new(
      Span::new(span.start + mod_lo as u32, span.start + mod_hi as u32),
      Str::from(text),
      Str::from(text),
    ));
    cursor = mod_hi;
  }

  if is_prop_shorthand && modifiers.is_empty() {
    let end = span.end;
    modifiers.push(VIdentifier::new(Span::new(end, end), Str::from(""), Str::from("")));
  }

  (
    ArenaBox::new_in(
      VAttributeKey::Directive(VDirectiveKey::new(
        span,
        name_ident,
        argument,
        modifiers,
        Str::from(raw),
      )),
      alloc,
    ),
    true,
  )
}

fn classify_arg(after: &str, base_in_raw: usize) -> (usize, &str, usize, bool) {
  if let Some(rest) = after.strip_prefix('[') {
    if let Some(end_inner) = rest.find(']') {
      let inner = &rest[..end_inner];
      let consumed = end_inner + 2;
      return (base_in_raw, inner, base_in_raw + consumed, true);
    }
    let dot = after.find('.').unwrap_or(after.len());
    return (base_in_raw, &after[..dot], base_in_raw + dot, false);
  }
  let dot = after.find('.').unwrap_or(after.len());
  (base_in_raw, &after[..dot], base_in_raw + dot, false)
}

/// Detect a `v-pre` attribute among raw start-tag tokens.
pub fn has_v_pre(attrs: &[AttrTok<'_>]) -> bool {
  attrs.iter().any(|a| a.key == "v-pre")
}
