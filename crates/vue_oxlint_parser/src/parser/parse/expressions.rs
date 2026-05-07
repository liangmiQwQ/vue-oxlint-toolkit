use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{Expression, Statement};
use oxc_span::Span;

use crate::ast::{VForExpression, VOnExpression, VSlotExpression};
use crate::parser::parse::TemplateParser;
use crate::parser::parse::utils::{split_v_for_expression, trimmed_sub_span};

impl<'a, 'b> TemplateParser<'_, 'a, 'b>
where
  'b: 'a,
{
  pub(super) fn parse_v_for_expression(
    &mut self,
    value_span: Span,
  ) -> Option<VForExpression<'a, 'b>> {
    let source = value_span.source_text(self.parser.source_text);
    let (left_source, right_source, operator_index) = split_v_for_expression(source)?;
    let left_span = trimmed_sub_span(value_span, left_source, source);
    let right_start = value_span.start
      + operator_index as u32
      + source[operator_index..].find(right_source)? as u32;
    let right_span = Span::new(right_start, right_start + right_source.len() as u32);

    let allocator = Allocator::new();
    let left_trimmed = left_source.trim();
    let (mut expression, _, left_tokens) =
      if left_trimmed.starts_with('(') && left_trimmed.ends_with(')') {
        // SAFETY: this wrapper forms an arrow function with the v-for aliases as params.
        unsafe { self.parser.parse_expression(left_span, b"(", b"=>0)", &allocator)? }
      } else {
        // SAFETY: this wrapper forms an arrow function with one v-for alias param.
        unsafe { self.parser.parse_expression(left_span, b"((", b")=>0)", &allocator)? }
      };

    let Expression::ArrowFunctionExpression(arrow) = &mut expression else {
      return None;
    };

    let params = arrow.params.clone_in(self.parser.js_allocator);
    let (right, references, tokens) = self.parser.parse_pure_expression(right_span)?;
    if !left_tokens.is_empty() {
      self.parser.sfc.template_tokens.push(left_tokens.into());
    }
    let operator_start = value_span.start + operator_index as u32 + 1;
    let operator = &source[operator_index + 1..operator_index + 3];
    let token_type = if operator == "in" {
      "Keyword"
    } else {
      // vue-eslint-parser exposes `of` as an Identifier in v-for values.
      "Identifier"
    };
    let operator_token = format!(
      r#"{{"type":"{token_type}","value":"{operator}","start":{operator_start},"end":{}}}"#,
      operator_start + operator.len() as u32,
    );
    let operator_token = self.alloc_value(&operator_token);
    self.parser.sfc.template_tokens.push(operator_token.into());
    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }

    Some(VForExpression { left: params, right, references, span: value_span })
  }

  pub(super) fn parse_v_slot_expression(
    &mut self,
    value_span: Span,
  ) -> Option<VSlotExpression<'b>> {
    let allocator = Allocator::new();
    // SAFETY: this wrapper forms an arrow function with slot props as params.
    let (mut expression, _, tokens) =
      unsafe { self.parser.parse_expression(value_span, b"((", b")=>0)", &allocator)? };
    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }
    let Expression::ArrowFunctionExpression(arrow) = &mut expression else {
      return None;
    };

    Some(VSlotExpression {
      params: arrow.params.clone_in(self.parser.js_allocator),
      span: value_span,
    })
  }

  pub(super) fn parse_v_on_expression(
    &mut self,
    value_span: Span,
  ) -> Option<VOnExpression<'a, 'b>> {
    let allocator = Allocator::new();
    let ret = self.parser.oxc_parse(value_span, b"{", b"}", Some(&allocator))?;
    let Some(Statement::BlockStatement(block)) = ret.statements.into_iter().next() else {
      return None;
    };

    Some(VOnExpression {
      body: block.body.clone_in(self.parser.js_allocator),
      references: ret.references,
      span: value_span,
    })
  }
}
