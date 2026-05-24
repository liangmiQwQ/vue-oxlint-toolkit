//! The definition of JavaScript related nodes
//! Most of these nodes need needs 'b lifetime to make js related nodes storage in `js_allocator`
//!
//! ## Expressions
//!
//! We split `VExpressionContainer` into different kinds, but they are all serialized as `VExpressionContainer`
//! - `VInterpolation`: `{{ }}`, can be treated as a `VNode`.
//! - `VDirectiveExpression`: `v-bind`, `v-model`, `v-if` and most of the directives (including all custom directives).
//! - `VOnExpression`: `v-on`, used in `v-on` directive, with body to storage statements inside.
//! - `VForExpression`: `v-for`, used in `v-for` directive, with left and right to storage variable definitions and source.
//! - `VSlotExpression`: `v-slot`, used in `v-slot` directive, with params to storage variable definitions.
//!
//! ## Pure JS
//!
//! It's only used in <script> and <script setup> blocks.

use oxc_allocator::{Box, Vec};
use oxc_ast::ast::{Directive, Expression, FormalParameters, Statement};
use oxc_estree::{
  Concat2, ESTree, JsonSafeString, SequenceSerializer, Serializer, StructSerializer,
};
use oxc_span::Span;

#[derive(Debug)]
pub struct VInterpolation<'b> {
  pub expression: Expression<'b>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VDirectiveExpression<'b> {
  pub expression: Expression<'b>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VDirectiveArgumentExpression<'b> {
  pub expression: Expression<'b>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VOnExpression<'b> {
  pub body: Vec<'b, Statement<'b>>,
  pub expression_span: Span,
  pub span: Span,
}

#[derive(Debug)]
pub struct VForExpression<'b> {
  pub left: Box<'b, FormalParameters<'b>>,
  pub right: Expression<'b>,
  pub expression_span: Span,
  pub span: Span,
}

#[derive(Debug)]
pub struct VSlotExpression<'b> {
  pub params: Box<'b, FormalParameters<'b>>,
  pub expression_span: Span,
  pub span: Span,
}

#[derive(Debug)]
pub struct VPureScript<'b> {
  pub statements: Vec<'b, Statement<'b>>,
  pub directives: Vec<'b, Directive<'b>>,
  pub setup: bool,
  pub span: Span,
}

impl ESTree for VInterpolation<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VExpressionContainer"));
    state.serialize_field("expression", &VueExpression(&self.expression));
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VDirectiveExpression<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VExpressionContainer"));
    state.serialize_field("expression", &VueExpression(&self.expression));
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VDirectiveArgumentExpression<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VExpressionContainer"));
    state.serialize_field("expression", &VueExpression(&self.expression));
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VOnExpression<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    // Define a temporary struct to hold the expression
    struct VOnExpression<'b> {
      body: &'b Vec<'b, Statement<'b>>,
      span: Span,
    }
    impl ESTree for VOnExpression<'_> {
      fn serialize<S: Serializer>(&self, serializer: S) {
        let mut state = serializer.serialize_struct();
        state.serialize_field("type", &JsonSafeString("VOnExpression"));
        state.serialize_field("body", self.body);
        state.serialize_span(self.span);
        state.end();
      }
    }

    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VExpressionContainer"));
    state.serialize_field(
      "expression",
      &VOnExpression { body: &self.body, span: self.expression_span },
    );
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VForExpression<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    // Define a temporary struct to hold the expression
    struct VForExpression<'b> {
      left: &'b FormalParameters<'b>,
      right: &'b Expression<'b>,
      span: Span,
    }
    impl ESTree for VForExpression<'_> {
      fn serialize<S: Serializer>(&self, serializer: S) {
        let mut state = serializer.serialize_struct();
        state.serialize_field("type", &JsonSafeString("VForExpression"));
        state.serialize_field("left", &self.left);
        state.serialize_field("right", &self.right);
        state.serialize_span(self.span);
        state.end();
      }
    }

    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VExpressionContainer"));
    state.serialize_field(
      "expression",
      &VForExpression { left: &self.left, right: &self.right, span: self.expression_span },
    );
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VSlotExpression<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    // Define a temporary struct to hold the expression
    struct VSlotExpression<'b> {
      params: &'b FormalParameters<'b>,
      span: Span,
    }
    impl ESTree for VSlotExpression<'_> {
      fn serialize<S: Serializer>(&self, serializer: S) {
        let mut state = serializer.serialize_struct();
        state.serialize_field("type", &JsonSafeString("VSlotScopeExpression"));
        state.serialize_field("params", &self.params);
        state.serialize_span(self.span);
        state.end();
      }
    }

    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VExpressionContainer"));
    state.serialize_field(
      "expression",
      &VSlotExpression { params: &self.params, span: self.expression_span },
    );
    state.serialize_span(self.span);
    state.end();
  }
}

// Will be transformed in toolkit side
impl ESTree for VPureScript<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VPureScript"));
    if self.setup {
      state.serialize_field("body", &SetupScriptBody(self));
    } else {
      state.serialize_field("body", &Concat2(&self.directives, &self.statements));
    }
    state.serialize_span(self.span);
    state.end();
  }
}

struct SetupScriptBody<'b>(&'b VPureScript<'b>);

impl ESTree for SetupScriptBody<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut seq = serializer.serialize_sequence();
    for directive in &self.0.directives {
      seq.serialize_element(&SetupDirectiveStatement(directive));
    }
    for statement in &self.0.statements {
      seq.serialize_element(statement);
    }
    seq.end();
  }
}

struct SetupDirectiveStatement<'b>(&'b Directive<'b>);

impl ESTree for SetupDirectiveStatement<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("ExpressionStatement"));
    state.serialize_field("expression", &self.0.expression);
    state.serialize_ts_field("directive", &());
    state.serialize_span(self.0.span);
    state.end();
  }
}

struct VueExpression<'b>(&'b Expression<'b>);

impl ESTree for VueExpression<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    match self.0 {
      Expression::ParenthesizedExpression(expression) => {
        expression.expression.serialize(serializer);
      }
      expression => expression.serialize(serializer),
    }
  }
}
