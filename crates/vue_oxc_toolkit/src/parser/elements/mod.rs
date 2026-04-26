use oxc_allocator::{Allocator, CloneIn, TakeIn, Vec as ArenaVec};
use oxc_ast::{
  Comment, CommentKind, NONE,
  ast::{Expression, JSXAttributeItem, JSXChild, JSXExpression, PropertyKind, Statement},
};
use oxc_span::{GetSpanMut, SPAN, Span};
use vize_armature::{
  CommentNode, DirectiveNode, ElementNode, InterpolationNode, PropNode, TemplateChildNode, TextNode,
};

use crate::{
  is_void_tag,
  parser::{
    ParserImpl,
    elements::{
      v_for::VForWrapper,
      v_if::{VIf, VIfManager},
      v_slot::VSlotWrapper,
    },
    error,
  },
  utils::{AttributeExt, DirectiveExt, ElementExt, VizeSpan, is_dynamic_arg, kebab_to_case},
};

mod directive;
mod v_for;
mod v_if;
mod v_slot;

impl<'a: 'b, 'b> ParserImpl<'a> {
  fn parse_children(
    &mut self,
    _start: u32,
    _end: u32,
    children: &[TemplateChildNode<'_>],
  ) -> ArenaVec<'a, JSXChild<'a>> {
    let ast = self.ast;
    if children.is_empty() {
      return ast.vec();
    }
    let mut result = ast.vec_with_capacity(children.len());
    let mut v_if_manager = VIfManager::new(&ast);

    for child in children {
      match child {
        TemplateChildNode::Element(node) => {
          let (child, v_if) = self.parse_element_ref(node, None);
          if let Some(v_if) = v_if {
            if let Some(child) = self.add_v_if(child, v_if, &mut v_if_manager) {
              result.push(child);
            }
          } else {
            if let Some(chain) = v_if_manager.take_chain() {
              result.push(chain);
            }
            result.push(child);
          }
        }
        TemplateChildNode::Text(text) => result.push(self.parse_text(text)),
        TemplateChildNode::Comment(comment) => result.push(self.parse_comment(comment)),
        TemplateChildNode::Interpolation(interp) => result.push(self.parse_interpolation(interp)),
        // If/For/TextCall etc. only appear after Vue's transform pipeline; we
        // operate on the raw parser output and never see them here.
        _ => {}
      }
    }

    if let Some(chain) = v_if_manager.take_chain() {
      result.push(chain);
    }

    result
  }

  pub fn parse_element_ref(
    &mut self,
    node: &ElementNode<'_>,
    children: Option<ArenaVec<'a, JSXChild<'a>>>,
  ) -> (JSXChild<'a>, Option<VIf<'a>>) {
    let ast = self.ast;
    let tag_name = node.tag.as_str();

    let open_element_span = {
      let start = node.loc.start.offset;
      let tag_name_end = node
        .props
        .last()
        .map_or_else(|| start + 1 + tag_name.len() as u32, |prop| prop.loc().end.offset);
      let end = memchr::memchr(b'>', &self.source_text.as_bytes()[tag_name_end as usize..])
        .map(|i| tag_name_end + i as u32 + 1)
        .unwrap(); // SAFETY: the tag must be closed, otherwise vize would have panicked.
      Span::new(start, end)
    };

    let (location_span, end_element_span) = if node.is_self_closing || is_void_tag!(tag_name) {
      (node.loc.span(), node.loc.span())
    } else {
      let close_span =
        crate::utils::element_close_span(self.source_text, node.loc.end.offset, tag_name);
      (Span::new(node.loc.start.offset, close_span.end), close_span)
    };

    let allocator = Allocator::new();
    let mut element_name = {
      let name_span = node.name_span();

      if tag_name.contains('.')
        && let Some(expr) = unsafe {
          let original_source_type = self.source_type;
          self.source_type = self.source_type.with_jsx(true);

          // Defer to oxc_parser directly — handling `<a.b.c.d />` ourselves
          // would mean re-implementing JSXMemberExpression parsing.
          // SAFETY: use `()` as wrap
          let expr = self.parse_expression(name_span, b"(<", b"/>)", &allocator);

          self.source_type = original_source_type;

          expr
        }
        && let Expression::JSXElement(mut jsx_element) = expr
      {
        jsx_element.opening_element.name.take_in(self.allocator)
      } else if tag_name.contains('-') {
        let name = kebab_to_case(tag_name, true);
        ast.jsx_element_name_identifier_reference(name_span, ast.str(&name))
      } else {
        let name = ast.str(tag_name);
        if node.is_component_like() {
          ast.jsx_element_name_identifier_reference(name_span, name)
        } else {
          ast.jsx_element_name_identifier(name_span, name)
        }
      }
    }
    .clone_in(self.allocator);

    let mut v_for_wrapper = VForWrapper::new(&ast);
    let mut v_slot_wrapper = VSlotWrapper::new(&ast);
    let mut v_if_state: Option<VIf<'a>> = None;
    let mut attributes = ast.vec();
    for prop in &node.props {
      attributes.push(self.parse_prop(
        prop,
        &mut v_for_wrapper,
        &mut v_slot_wrapper,
        &mut v_if_state,
      ));
    }

    let children = children.unwrap_or_else(|| {
      v_slot_wrapper.wrap(self.parse_children(
        open_element_span.end,
        end_element_span.start,
        &node.children,
      ))
    });

    let opening_element_name = element_name.clone_in(self.allocator);

    let closing_element = if node.is_self_closing {
      Some(ast.jsx_closing_element(SPAN, ast.jsx_element_name_identifier(SPAN, ast.str(""))))
    } else if is_void_tag!(tag_name) {
      None
    } else {
      Some(ast.jsx_closing_element(end_element_span, {
        let span = Span::sized(end_element_span.start + 2, tag_name.len() as u32);
        *element_name.span_mut() = span;
        element_name
      }))
    };

    (
      v_for_wrapper.wrap(ast.jsx_element(
        location_span,
        ast.jsx_opening_element(open_element_span, opening_element_name, NONE, attributes),
        children,
        closing_element,
      )),
      v_if_state,
    )
  }

  fn parse_prop(
    &mut self,
    prop: &PropNode<'_>,
    v_for_wrapper: &mut VForWrapper<'_, 'a>,
    v_slot_wrapper: &mut VSlotWrapper<'_, 'a>,
    v_if_state: &mut Option<VIf<'a>>,
  ) -> JSXAttributeItem<'a> {
    let ast = self.ast;
    match prop {
      PropNode::Attribute(attr) => {
        let attr_span = attr.full_span(self.source_text);
        ast.jsx_attribute_item_attribute(
          attr_span,
          ast.jsx_attribute_name_identifier(
            attr.name_loc.span(),
            ast.str(attr.name_loc.span().source_text(self.source_text)),
          ),
          attr.value.as_ref().map(|value| {
            // vize TextNode.loc covers just the value content (without quotes),
            // which is exactly what oxc's StringLiteral wants.
            let value_span = value.loc.span();
            ast.jsx_attribute_value_string_literal(
              value_span,
              ast.str(value_span.source_text(self.source_text)),
              None,
            )
          }),
        )
      }
      PropNode::Directive(dir) => {
        self.parse_directive_prop(dir, v_for_wrapper, v_slot_wrapper, v_if_state)
      }
    }
  }

  fn parse_directive_prop(
    &mut self,
    dir: &DirectiveNode<'_>,
    v_for_wrapper: &mut VForWrapper<'_, 'a>,
    v_slot_wrapper: &mut VSlotWrapper<'_, 'a>,
    v_if_state: &mut Option<VIf<'a>>,
  ) -> JSXAttributeItem<'a> {
    let ast = self.ast;
    let dir_span = dir.full_span(self.source_text);
    let dir_name = self.parse_directive_name(dir);

    // Side-effects on wrappers — these need to run regardless of how we render
    // the directive value.
    match dir.name.as_str() {
      "slot" => self.analyze_v_slot(dir, v_slot_wrapper, &dir_name),
      "for" => self.analyze_v_for(dir, v_for_wrapper),
      "else" => *v_if_state = Some(VIf::Else),
      _ => {}
    }

    if matches!(dir.name.as_str(), "if" | "else-if") && dir.exp.is_none() {
      error::v_if_else_without_expression(&mut self.errors, dir_span);
    }

    // `v-bind="expr"` (and `:="expr"`) — argument-less binding compiles to
    // a JSX spread attribute, mirroring Vue's `<div v-bind="obj" />`
    // behavior. See https://play.vuejs.org/#eNqVkbtOwzAUhl/FOkuWNC2CKQqVAFWiDICA0UuID8HFsS1f0khR3h3bVS9DVamb/V/s7+iM8KB10XuEEiqHnRa1wyWVhFSM96SfffPJ7imMhLOSZLXWWU4aUVsbbtvZzWKRkYnCkjyvSTUPlWO3vLJWzU/+hxycbZT84W2xsUoGvDG+TKFRneYCzZt2XElLoSTJiV4thNq+JM0Zj/leb36x+Tujb+wQNQrvBi2aHikcPFebFt3OXn2+4hDOB7NTzIuQvmB+oFXCR8Zd7NFLFrBPcol23WllHJftl10NDqXdDxVBY3JKeQphR08XRj/i3hZ3qUflBNM/rC6XVg==
    if dir.name.as_str() == "bind"
      && dir.arg.is_none()
      && let Some(exp) = &dir.exp
      && let Some(argument) = self.parse_pure_expression(exp.span())
    {
      return ast.jsx_attribute_item_spread_attribute(dir_span, argument);
    }

    // Vue 3.4+ same-name shorthand (`:foo`, `:msg-id`): vize synthesizes
    // `dir.exp` with `dir.shorthand == true` and `exp.content` already
    // camelized. The synthesized expression doesn't correspond to a real
    // source range, so we emit a dummy span here.
    if dir.shorthand
      && let Some(vize_armature::ExpressionNode::Simple(s)) = dir.exp.as_ref()
    {
      let ident = ast.str(s.content.as_str());
      return ast.jsx_attribute_item_attribute(
        dir_span,
        dir_name,
        Some(ast.jsx_attribute_value_expression_container(
          SPAN,
          JSXExpression::from(ast.expression_identifier(SPAN, ident)),
        )),
      );
    }

    let value = if let Some(exp) = &dir.exp {
      let expr_span = exp.span();
      Some(
        ast.jsx_attribute_value_expression_container(
          // The container span starts one byte before the expression — the
          // opening quote — and runs to the directive end so JSX renders the
          // surrounding `="..."` form.
          Span::new(expr_span.start.saturating_sub(1), dir_span.end),
          self
            .directive_value_expression(dir, expr_span, v_if_state)
            .unwrap_or_else(|| JSXExpression::EmptyExpression(ast.jsx_empty_expression(SPAN))),
        ),
      )
    } else if let Some(arg) = &dir.arg
      && is_dynamic_arg(arg)
      && let Some(argument) =
        self.parse_dynamic_argument(dir, ast.expression_identifier(SPAN, "undefined"))
    {
      // v-slot:[name] / v-bind:[name] without a value
      Some(ast.jsx_attribute_value_expression_container(SPAN, argument.into()))
    } else if dir_span.end > dir.loc.end.offset {
      // Empty quoted value such as `v-for=""`. The directive has a value
      // delimiter but nothing inside; emit an empty expression container so
      // the JSX surface reflects the source.
      let container_span = Span::new(dir_span.end - 2, dir_span.end);
      Some(ast.jsx_attribute_value_expression_container(
        container_span,
        JSXExpression::EmptyExpression(ast.jsx_empty_expression(SPAN)),
      ))
    } else {
      None
    };

    ast.jsx_attribute_item_attribute(dir_span, dir_name, value)
  }

  /// Build the `JSXExpression` that lives inside a directive's
  /// `="..."` value container, dispatching on directive name.
  fn directive_value_expression(
    &mut self,
    dir: &DirectiveNode<'_>,
    expr_span: Span,
    v_if_state: &mut Option<VIf<'a>>,
  ) -> Option<JSXExpression<'a>> {
    // v-for / v-slot / v-else have their expression captured by their
    // wrapper / state, so we don't render it directly.
    if matches!(dir.name.as_str(), "for" | "slot" | "else") {
      return None;
    }

    let expr = self.parse_pure_expression(expr_span);
    match dir.name.as_str() {
      "if" => {
        *v_if_state = expr.map(VIf::If);
        None
      }
      "else-if" => {
        *v_if_state = expr.map(VIf::ElseIf);
        None
      }
      _ => Some(JSXExpression::from(self.parse_dynamic_argument(dir, expr?)?)),
    }
  }

  fn parse_dynamic_argument(
    &mut self,
    dir: &DirectiveNode<'_>,
    expression: Expression<'a>,
  ) -> Option<Expression<'a>> {
    let head_span = dir.head_span(self.source_text);
    let head_name = head_span.source_text(self.source_text);
    let dir_start = dir.loc.start.offset;
    if let Some(arg) = &dir.arg
      && is_dynamic_arg(arg)
    {
      let arg_loc = arg.loc();
      // For `v-bind:[arg]` the dynamic arg starts at `[` which is two bytes
      // past the directive name; for the shorthand `:[arg]` it's two bytes
      // past the start of the directive itself.
      let dynamic_arg_start = if head_name.starts_with("v-") {
        dir_start + 2 + dir.name.len() as u32 + 2
      } else {
        dir_start + 2
      };
      let dynamic_arg_expression = self.parse_pure_expression(Span::sized(
        dynamic_arg_start,
        arg_loc.end.offset - arg_loc.start.offset,
      ))?;

      Some(self.ast.expression_object(
        SPAN,
        self.ast.vec1(self.ast.object_property_kind_object_property(
          SPAN,
          PropertyKind::Init,
          dynamic_arg_expression.into(),
          expression,
          false,
          false,
          true,
        )),
      ))
    } else {
      Some(expression)
    }
  }

  fn parse_text(&self, text: &TextNode) -> JSXChild<'a> {
    let span = text.loc.span();
    let raw = self.ast.str(span.source_text(self.source_text));
    self.ast.jsx_child_text(span, raw, Some(raw))
  }

  fn parse_comment(&mut self, comment: &CommentNode) -> JSXChild<'a> {
    let ast = self.ast;
    let span = comment.loc.span();
    let content = span.source_text(self.source_text);
    self.comments.push(Comment::new(
      span.start + 1,
      span.end - 1,
      if content.contains('\n') {
        CommentKind::MultiLineBlock
      } else {
        CommentKind::SingleLineBlock
      },
    ));
    ast.jsx_child_expression_container(span, ast.jsx_expression_empty_expression(SPAN))
  }

  fn parse_interpolation(&mut self, introp: &InterpolationNode<'_>) -> JSXChild<'a> {
    let ast = self.ast;
    let container_span = introp.loc.span();
    // vize InterpolationNode.content.loc() gives the inner expression span
    // (without the enclosing `{{` / `}}`).
    let expr_span = introp.content.span();

    ast.jsx_child_expression_container(
      container_span,
      self
        .parse_pure_expression(expr_span)
        .map_or_else(|| ast.jsx_expression_empty_expression(SPAN), JSXExpression::from),
    )
  }

  pub fn parse_pure_expression(&mut self, span: Span) -> Option<Expression<'a>> {
    let allocator = Allocator::new();
    // SAFETY: use `()` as wrap
    unsafe { self.parse_expression(span, b"(", b")", &allocator).clone_in(self.allocator) }
  }

  /// Parse expression with [`oxc_parser`].
  ///
  /// We parse manually rather than calling [`oxc_parser::Parser::parse_expression`]
  /// to keep code comments collected during parsing.
  ///
  /// ## Safety
  /// - `start_wrap` must start with `(`
  /// - `end_wrap` must end with `)`
  pub unsafe fn parse_expression(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: &'b Allocator,
  ) -> Option<Expression<'b>> {
    let (_, mut body, _) = self.oxc_parse(span, start_wrap, end_wrap, Some(allocator))?;

    let Some(Statement::ExpressionStatement(stmt)) = body.get_mut(0) else {
      // SAFETY: We always wrap the source in parentheses, so it should always be an expression statement.
      unreachable!()
    };
    let Expression::ParenthesizedExpression(expression) = &mut stmt.expression else {
      // SAFETY: We always wrap the source in parentheses, so it should always be a parenthesized expression.
      unreachable!()
    };
    Some(expression.expression.take_in(self.allocator))
  }
}
