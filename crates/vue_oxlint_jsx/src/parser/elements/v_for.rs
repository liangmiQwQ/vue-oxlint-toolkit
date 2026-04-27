use oxc_allocator::{Allocator, CloneIn, TakeIn};
use oxc_ast::{
  AstBuilder, NONE,
  ast::{
    Argument, Expression, FormalParameters, JSXChild, JSXElement, JSXExpression,
    ParenthesizedExpression,
  },
};

use oxc_span::{SPAN, Span};
use regex::Regex;
use vue_compiler_core::parser::Directive;

use crate::parser::{ParserImpl, error, parse::SourceLocatonSpan};

pub struct VForWrapper<'a, 'b> {
  ast: &'a AstBuilder<'b>,
  data_origin: Option<ParenthesizedExpression<'b>>,
  params: Option<FormalParameters<'b>>,
}

impl<'a> ParserImpl<'a> {
  fn invalid_v_for_expression(&mut self, span: Span) -> Option<()> {
    error::invalid_v_for_expression(&mut self.errors, span);
    None
  }

  pub fn analyze_v_for(&mut self, dir: &Directive<'a>, wrapper: &mut VForWrapper<'_, 'a>) {
    (|| {
      if dir.has_empty_expr() {
        self.invalid_v_for_expression(dir.location.span())?;
      }
      let expr = dir.expression.as_ref().unwrap(); // SAFETY: Checked above

      // https://github.com/vuejs/core/blob/e1ccd9fde8f57fe7bd40fdf1345692ab3e6a1fa0/packages/compiler-core/src/utils.ts#L571
      let for_alias_regex = Regex::new(r"^([\s\S]*?)\s+(?:in|of)\s+(\S[\s\S]*)").unwrap();
      if let Some(caps) = for_alias_regex.captures(expr.content.raw)
        && let Some(cap1) = caps.get(1)
        && let Some(cap2) = caps.get(2)
      {
        let start = expr.location.span().start + 1;
        wrapper.set_data_origin(self.ast.parenthesized_expression(
          SPAN,
          self.parse_pure_expression(Span::new(
            start + cap2.start() as u32,
            start + cap2.end() as u32,
          ))?,
        ));

        let span = Span::new(start + cap1.start() as u32, start + cap1.end() as u32);
        let params = cap1.as_str();
        let allocator = Allocator::new();
        let (mut expr, should_dummy_span) =
          if params.trim().starts_with('(') && params.trim().ends_with(')') {
            // SAFETY: use `()` as wrap
            let expr = unsafe { self.parse_expression(span, b"(", b"=>0)", &allocator)? };
            (expr, false)
          } else {
            // SAFETY: use `(` and `)` as wrap
            let expr = unsafe { self.parse_expression(span, b"((", b")=>0)", &allocator)? };
            (expr, true)
          };

        let Expression::ArrowFunctionExpression(expression) = &mut expr else {
          unreachable!();
        };

        let mut params = expression.params.take_in(self.ast.allocator);
        if should_dummy_span {
          params.span = SPAN;
        }

        wrapper.set_params(params.clone_in(self.allocator));
      } else {
        self.invalid_v_for_expression(dir.location.span())?;
      }

      Some(())
    })();
  }
}

/// Wrap the JSX element with a function call, similar to jsx {items.map(items => <div key={item.id} />)} but with vue semantic.
impl<'a, 'b> VForWrapper<'a, 'b> {
  pub const fn new(ast: &'a AstBuilder<'b>) -> Self {
    Self { ast, data_origin: None, params: None }
  }

  pub fn wrap(self, element: JSXElement<'b>) -> JSXChild<'b> {
    if self.include_v_for() {
      let Self { ast, data_origin, params } = self;
      let data_origin = data_origin.unwrap();
      let params = params.unwrap();

      ast.jsx_child_expression_container(
        SPAN,
        JSXExpression::CallExpression(ast.alloc_call_expression(
          SPAN,
          Expression::ParenthesizedExpression(ast.alloc(data_origin)),
          NONE,
          self.ast.vec1(Argument::ArrowFunctionExpression(ast.alloc_arrow_function_expression(
            SPAN,
            true,
            false,
            NONE,
            params,
            NONE,
            ast.function_body(
              SPAN,
              ast.vec(),
              ast.vec1(ast.statement_expression(
                SPAN,
                ast.expression_parenthesized(SPAN, Expression::JSXElement(self.ast.alloc(element))),
              )),
            ),
          ))),
          false,
        )),
      )
    } else {
      JSXChild::Element(self.ast.alloc(element))
    }
  }
}

impl<'b> VForWrapper<'_, 'b> {
  const fn include_v_for(&self) -> bool {
    self.data_origin.is_some() && self.params.is_some()
  }

  const fn set_data_origin(&mut self, data_origin: ParenthesizedExpression<'b>) {
    self.data_origin = Some(data_origin);
  }

  const fn set_params(&mut self, params: FormalParameters<'b>) {
    self.params = Some(params);
  }
}

#[cfg(test)]
mod tests {
  use crate::test_ast;

  #[test]
  fn v_for() {
    test_ast!("directive/v-for.vue");
  }

  #[test]
  fn v_for_error() {
    test_ast!("directive/v-for-error.vue", true, false);
  }
}
