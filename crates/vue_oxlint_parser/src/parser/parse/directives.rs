use std::sync::LazyLock;

use crate::VueParser;
use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};
use regex::Regex;

static FOR_ALIAS_RE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^([\s\S]*?)\s+(in|of)\s+(\S[\s\S]*)").unwrap());

#[derive(Debug, Clone, Copy)]
struct VForParts<'s> {
  left_start: usize,
  left_end: usize,
  operator: &'s str,
  operator_start: usize,
  operator_end: usize,
  right_start: usize,
  right_end: usize,
}

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub(super) fn emit_expression_tokens(&mut self, start: usize, end: usize) {
    let span = Span::new(start as u32, end as u32);
    if let Some((_, tokens)) = self.parse_pure_expression(span)
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  pub(super) fn emit_handler_tokens(&mut self, start: usize, end: usize) {
    let span = Span::new(start as u32, end as u32);
    let allocator = Allocator::new();
    let tokens = unsafe {
      self.parse_expression(span, b"(()=>{", b"})", &allocator, |expression| {
        let Expression::ArrowFunctionExpression(arrow) = expression else {
          return None;
        };

        let Some(first) = arrow.body.statements.first() else {
          return Some(((), Span::new(span.start, span.start)));
        };
        let last = arrow.body.statements.last().unwrap_or(first);

        Some(((), Span::new(first.span().start, last.span().end)))
      })
    };

    if let Some(((), tokens)) = tokens
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  pub(super) fn emit_slot_params_tokens(&mut self, start: usize, end: usize) {
    let start = self.skip_ws(start, end);
    let end = self.trim_end_ws(start, end);
    if start >= end {
      return;
    }

    let span = Span::new(start as u32, end as u32);
    let is_parenthesized = self.byte(start) == b'(' && self.byte(end - 1) == b')';
    let (start_wrap, end_wrap): (&[u8], &[u8]) =
      if is_parenthesized { (b"(", b"=>0)") } else { (b"((", b")=>0)") };

    let allocator = Allocator::new();
    let tokens = unsafe {
      self.parse_expression(span, start_wrap, end_wrap, &allocator, |expression| {
        let Expression::ArrowFunctionExpression(arrow) = expression else {
          return None;
        };

        // Mirrors the v-slot arrow-wrapper trick from the JSX parser.
        // https://play.vuejs.org/#eNp9kD1PwzAQhv+KdXNJB5iigASoAwyAgNFLlBxpir/kO4dIkf87tquGDsBmvc9z9utb4Na5agoINTSM2qmW8UYaIZp7q52YLkhZrvfY9uivJSxCI1E7oIgSiifEchbGMrrNs4k22/VK2ABTZ83HOFQHsia9t2RXQpfcUaF/djxaQxJqUUhmrVL267Fk7ANuTnm3x+7zl/xAc84kvHgk9BNKWBm3fkA+4t3bE87pvEJt+6CS/Q98RbIq5I5H7S6YPtU+80rbB+2s59EM77SbGQ2dPpWLZjMWX0Jael7TX1//qXtZXZU5aSLEbzFYjTA=
        let token_span = if is_parenthesized {
          span
        } else if let Some(first) = arrow.params.items.first() {
          let end = arrow.params.rest.as_ref().map_or_else(
            || arrow.params.items.last().map_or(first.span.end, |last| last.span.end),
            |rest| rest.span.end,
          );
          Span::new(first.span.start, end)
        } else if let Some(rest) = &arrow.params.rest {
          rest.span
        } else {
          Span::new(span.start, span.start)
        };

        Some(((), token_span))
      })
    };

    if let Some(((), tokens)) = tokens
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  pub(super) fn emit_v_for_tokens(&mut self, start: usize, end: usize) {
    let Some(parts) = self.split_v_for_expression(start, end) else {
      self.emit_expression_tokens(start, end);
      return;
    };

    self.emit_slot_params_tokens(parts.left_start, parts.left_end);
    self.push_manual_oxc_token(
      if parts.operator == "in" { "Keyword" } else { "Identifier" },
      parts.operator,
      parts.operator_start,
      parts.operator_end,
    );
    self.emit_expression_tokens(parts.right_start, parts.right_end);
  }

  fn split_v_for_expression(&self, start: usize, end: usize) -> Option<VForParts<'b>> {
    let source = &self.source_text[start..end];
    let caps = FOR_ALIAS_RE.captures(source)?;
    let left = caps.get(1)?;
    let operator = caps.get(2)?;
    let right = caps.get(3)?;

    Some(VForParts {
      left_start: start + left.start(),
      left_end: start + left.end(),
      operator: operator.as_str(),
      operator_start: start + operator.start(),
      operator_end: start + operator.end(),
      right_start: start + right.start(),
      right_end: start + right.end(),
    })
  }
}
