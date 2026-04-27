use std::mem::take;

use oxc_ast::{
  AstBuilder,
  ast::{Expression, JSXChild},
};

use oxc_span::{GetSpan, SPAN};

use crate::parser::{ParserImpl, error};

pub enum VIf<'a> {
  If(Expression<'a>),
  ElseIf(Expression<'a>),
  Else,
}

// We only use .into function.
#[allow(clippy::from_over_into)]
impl<'a> Into<Expression<'a>> for VIf<'a> {
  fn into(self) -> Expression<'a> {
    match self {
      VIf::If(e) | VIf::ElseIf(e) => e,
      // SAFETY: v-else should be processed as the last element
      VIf::Else => panic!("VIf::Else::into() called. v-else has no expression"),
    }
  }
}

// The manager of v-if / v-else-if / v-else, different from the wrapper, it works across multiple elements
pub struct VIfManager<'a, 'b> {
  ast: &'a AstBuilder<'b>,
  chain: Vec<(JSXChild<'b>, VIf<'b>)>, // child, v_if
}

impl<'a> ParserImpl<'a> {
  pub fn add_v_if(
    &mut self,
    child: JSXChild<'a>,
    v_if: VIf<'a>,
    manager: &mut VIfManager<'_, 'a>,
  ) -> Option<JSXChild<'a>> {
    if matches!(v_if, VIf::If(_)) {
      if manager.chain.is_empty() {
        manager.chain.push((child, v_if));
        None
      } else {
        // The previous v-if/v-else-if chain is finished
        let result = manager.take_chain();
        manager.chain.push((child, v_if));
        result
      }
    } else if manager.chain.is_empty() {
      // Orphan v-else-if / v-else
      // https://play.vuejs.org/#eNp9kLFuwjAQhl/FuhnC0E4ordRWDO3QVi2jlyg5gsGxLd85REJ5d2wjAgNis/7v8+m/O8Kbc0UfEJZQMnZOV4yv0ghRNqoX/Rw14VxtXiSwDyhBLCItF5MKM2CqrdmottiRNXHOMX2XUNvOKY3+x7GyhiQsRSaJVVrbw1fO0tjZJa+3WO/v5DsaUibh1yOh72ORiXHlW+QzXv1/4xDfE+xsE3S0H8A/JKtD6njW3oNpYu0bL7f97Jz1rEy7ptXAaOiyVL5LNMfsS4jH/Hiw+rXuU/Gc/0kzwngCD9Z/dQ==
      error::v_else_without_adjacent_if(&mut self.errors, child.span());
      Some(child)
    } else if matches!(v_if, VIf::Else) {
      manager.chain.push((child, v_if));
      // The chain is finished, return the result directly, for possible next node
      manager.take_chain()
    } else {
      manager.chain.push((child, v_if));
      None
    }
  }
}

impl<'a, 'b> VIfManager<'a, 'b> {
  pub const fn new(ast: &'a AstBuilder<'b>) -> Self {
    Self { ast, chain: vec![] }
  }

  pub fn take_chain(&mut self) -> Option<JSXChild<'b>> {
    if self.chain.is_empty() {
      // No chain exists
      return None;
    }
    let ast = self.ast;

    let mut chain_stack = take(&mut self.chain);

    // SAFETY: chain_stack is not empty
    let last = if matches!(chain_stack.last().unwrap().1, VIf::Else) {
      self.build_jsx_fragment_expression(chain_stack.pop().unwrap().0)
    } else {
      ast.expression_identifier(SPAN, "undefined")
    };

    let mut result = last;
    while let Some((child, v_if)) = chain_stack.pop() {
      result = ast.expression_conditional(
        SPAN,
        v_if.into(),
        self.build_jsx_fragment_expression(child),
        result,
      );
    }

    Some(ast.jsx_child_expression_container(SPAN, result.into()))
  }

  fn build_jsx_fragment_expression(&self, child: JSXChild<'b>) -> Expression<'b> {
    self.ast.expression_jsx_fragment(
      SPAN,
      self.ast.jsx_opening_fragment(SPAN),
      self.ast.vec1(child),
      self.ast.jsx_closing_fragment(SPAN),
    )
  }
}

#[cfg(test)]
mod tests {
  use crate::test_ast;

  #[test]
  fn v_if() {
    test_ast!("directive/v-if.vue");
    test_ast!("directive/v-if-error.vue", true, false);
  }
}
