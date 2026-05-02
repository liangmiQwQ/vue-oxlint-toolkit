//! Expression parsing: interpolations, directives, v-for, v-slot, v-on.

use oxc_allocator::CloneIn;
use oxc_ast::ast::{Expression, Statement};
use oxc_span::Span;
use regex::Regex;
use std::sync::OnceLock;

use crate::ast::{DirectiveExpression, DirectiveName, VForDirective, VSlotDirective};
use crate::parser::Parser;

fn for_alias_regex() -> &'static Regex {
  static RE: OnceLock<Regex> = OnceLock::new();
  RE.get_or_init(|| Regex::new(r"^([\s\S]*?)\s+(?:in|of)\s+(\S[\s\S]*)").unwrap())
}

impl<'a> Parser<'a> {
  /// Parse expression for interpolation `{{ expr }}`.
  /// The `expr_span` covers the raw expression text (without `{{` and `}}`).
  pub fn parse_expression_in_interpolation(&mut self, expr_span: Span) -> Option<Expression<'a>> {
    // Wrap as `(expr)` and unwrap the parenthesized expression
    // We need at least 1 byte before the span for '('
    if expr_span.start < 1 {
      return None;
    }

    // SAFETY: there's at least 1 byte before span.start (the '{' from '{{')
    // and at least 1 byte after span.end (the '}' from '}}')
    let result = unsafe { self.oxc_parse_with_wrap(expr_span, b"(", b")") };

    result.and_then(|(_, body, _)| extract_expression_from_body(self.allocator, &body))
  }

  /// Parse a pure expression (for v-bind, v-if, v-show, v-model, etc.)
  /// `value_span` is the span of the raw expression text (attribute value without quotes).
  pub fn parse_pure_expression(&mut self, value_span: Span) -> Option<Expression<'a>> {
    if value_span.start < 1 {
      return None;
    }

    // SAFETY: value starts after the opening quote character
    let result = unsafe { self.oxc_parse_with_wrap(value_span, b"(", b")") };

    result.and_then(|(_, body, _)| extract_expression_from_body(self.allocator, &body))
  }

  /// Parse v-for expression `(item, index) in list`.
  fn parse_v_for_expression(&mut self, value_span: Span) -> Option<DirectiveExpression<'a>> {
    let raw = self.source_text[value_span.start as usize..value_span.end as usize].to_string();
    let caps = for_alias_regex().captures(&raw)?;

    let lhs_match = caps.get(1)?;
    let rhs_match = caps.get(2)?;

    let base = value_span.start;
    let lhs_span = Span::new(base + lhs_match.start() as u32, base + lhs_match.end() as u32);
    let rhs_span = Span::new(base + rhs_match.start() as u32, base + rhs_match.end() as u32);

    // Parse RHS as a pure expression
    let right = self.parse_pure_expression(rhs_span)?;

    // Parse LHS as binding patterns via arrow function wrap
    let lhs_str = lhs_match.as_str().trim();

    // If lhs already has parens, wrap as `(LHS=>0)`, otherwise `((LHS)=>0)`
    let (start_wrap, end_wrap): (&[u8], &[u8]) =
      if lhs_str.starts_with('(') && lhs_str.ends_with(')') {
        if lhs_span.start < 1 {
          return None;
        }
        (b"(", b")=>0)")
      } else {
        if lhs_span.start < 2 {
          return None;
        }
        (b"((", b")=>0)")
      };

    // SAFETY: we checked bounds above
    let result = unsafe { self.oxc_parse_with_wrap(lhs_span, start_wrap, end_wrap) };

    let left =
      result.and_then(|(_, body, _)| extract_arrow_params_as_patterns(self.allocator, &body))?;

    Some(DirectiveExpression::For(VForDirective { left, right }))
  }

  /// Parse v-slot expression `(props)` as formal parameters.
  fn parse_v_slot_expression(&mut self, value_span: Span) -> Option<DirectiveExpression<'a>> {
    let raw =
      self.source_text[value_span.start as usize..value_span.end as usize].trim().to_string();

    if raw.is_empty() {
      return Some(DirectiveExpression::Slot(VSlotDirective { params: None }));
    }

    // Wrap as `((props)=>0)` to get arrow function parameters
    let (start_wrap, end_wrap): (&[u8], &[u8]) = if raw.starts_with('(') && raw.ends_with(')') {
      if value_span.start < 1 {
        return None;
      }
      (b"(", b"=>0)")
    } else {
      if value_span.start < 2 {
        return None;
      }
      (b"((", b")=>0)")
    };

    // SAFETY: we checked bounds above
    let result = unsafe { self.oxc_parse_with_wrap(value_span, start_wrap, end_wrap) };

    let params = result.and_then(|(_, body, _)| extract_arrow_params(self.allocator, &body));

    Some(DirectiveExpression::Slot(VSlotDirective { params }))
  }

  /// Parse v-on statement-list expression `{ stmts }` or just `expr`.
  fn parse_v_on_expression(&mut self, value_span: Span) -> Option<DirectiveExpression<'a>> {
    if value_span.start < 1 {
      return None;
    }

    // SAFETY: value starts after quote char
    let result = unsafe { self.oxc_parse_with_wrap(value_span, b"{", b"}") };

    let stmts = result.map(|(_, body, _)| {
      let mut stmts = Vec::new();
      for s in &body {
        if let Statement::BlockStatement(block) = s {
          for inner in &block.body {
            stmts.push(inner.clone_in(self.allocator));
          }
        } else {
          stmts.push(s.clone_in(self.allocator));
        }
      }
      stmts
    });

    stmts.map(DirectiveExpression::On)
  }

  /// Dispatch to the correct expression parser for a given directive.
  pub fn parse_directive_expression(
    &mut self,
    directive_name: &DirectiveName,
    value_span: Span,
    _value_raw: Option<&str>,
  ) -> Option<DirectiveExpression<'a>> {
    // Skip if span is trivially empty or has no content
    let raw = &self.source_text[value_span.start as usize..value_span.end as usize];
    if raw.trim().is_empty() {
      return None;
    }

    match directive_name {
      DirectiveName::For => self.parse_v_for_expression(value_span),
      DirectiveName::Slot => self.parse_v_slot_expression(value_span),
      DirectiveName::On => self.parse_v_on_expression(value_span),
      // All other directives: parse as pure expression
      DirectiveName::If
      | DirectiveName::ElseIf
      | DirectiveName::Show
      | DirectiveName::Model
      | DirectiveName::Bind
      | DirectiveName::Else
      | DirectiveName::Custom(_) => {
        self.parse_pure_expression(value_span).map(DirectiveExpression::Expression)
      }
    }
  }
}

fn unwrap_paren(expr: Expression<'_>) -> Expression<'_> {
  match expr {
    Expression::ParenthesizedExpression(paren) => paren.unbox().expression,
    other => other,
  }
}

/// Extract an expression from a statement body `[(expr)]`
fn extract_expression_from_body<'a>(
  allocator: &'a oxc_allocator::Allocator,
  body: &oxc_allocator::Vec<'a, Statement<'a>>,
) -> Option<Expression<'a>> {
  for stmt in body {
    if let Statement::ExpressionStatement(expr_stmt) = stmt {
      let expr = expr_stmt.expression.clone_in(allocator);
      return Some(unwrap_paren(expr));
    }
  }
  None
}

/// Extract binding patterns from arrow function params
fn extract_arrow_params_as_patterns<'a>(
  allocator: &'a oxc_allocator::Allocator,
  body: &oxc_allocator::Vec<'a, Statement<'a>>,
) -> Option<oxc_allocator::Vec<'a, oxc_ast::ast::BindingPattern<'a>>> {
  for stmt in body {
    if let Statement::ExpressionStatement(expr_stmt) = stmt
      && let Expression::ArrowFunctionExpression(arrow) = &expr_stmt.expression
    {
      let mut items = oxc_allocator::Vec::new_in(allocator);
      for param in &arrow.params.items {
        items.push(param.pattern.clone_in(allocator));
      }
      return Some(items);
    }
  }
  None
}

/// Extract formal parameters from arrow function
fn extract_arrow_params<'a>(
  allocator: &'a oxc_allocator::Allocator,
  body: &oxc_allocator::Vec<'a, Statement<'a>>,
) -> Option<oxc_ast::ast::FormalParameters<'a>> {
  for stmt in body {
    if let Statement::ExpressionStatement(expr_stmt) = stmt
      && let Expression::ArrowFunctionExpression(arrow) = &expr_stmt.expression
    {
      return Some((*arrow.params).clone_in(allocator));
    }
  }
  None
}
