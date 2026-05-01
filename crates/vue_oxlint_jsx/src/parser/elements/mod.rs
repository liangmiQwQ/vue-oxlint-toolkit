use oxc_allocator::{Allocator, CloneIn, TakeIn, Vec as ArenaVec};
use oxc_ast::{
  Comment, CommentKind, NONE,
  ast::{Expression, JSXAttributeItem, JSXChild, JSXExpression, PropertyKind, Statement},
};
use oxc_span::{GetSpanMut, SPAN, Span};
use vue_compiler_core::parser::{AstNode, Directive, DirectiveArg, ElemProp, Element, SourceNode};

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
    parse::SourceLocatonSpan,
  },
};

mod directive;
mod v_for;
mod v_if;
mod v_slot;

/// Convert kebab-case to camel-like case.
/// `pascal: true` → `PascalCase` (e.g. `keep-alive` → `KeepAlive`)
/// `pascal: false` → `camelCase`  (e.g. `msg-id` → `msgId`)
fn kebab_to_case(s: &str, pascal: bool) -> String {
  let mut result = String::with_capacity(s.len());
  let mut capitalize_next = pascal;
  for ch in s.chars() {
    if ch == '-' {
      capitalize_next = true;
    } else if capitalize_next {
      result.extend(ch.to_uppercase());
      capitalize_next = false;
    } else {
      result.push(ch);
    }
  }
  result
}

impl<'a: 'b, 'b> ParserImpl<'a> {
  fn parse_children(
    &mut self,
    _start: u32,
    _end: u32,
    children: Vec<AstNode<'a>>,
  ) -> ArenaVec<'a, JSXChild<'a>> {
    let ast = self.ast;
    if children.is_empty() {
      return ast.vec();
    }
    let mut result = self.ast.vec_with_capacity(children.len() + 2);

    let mut v_if_manager = VIfManager::new(&ast);
    for child in children {
      match child {
        AstNode::Element(node) => {
          let (child, v_if) = self.parse_element(node, None);

          if let Some(v_if) = v_if {
            if let Some(child) = self.add_v_if(child, v_if, &mut v_if_manager) {
              // There are three cases to return Some(child) for add_v_if function
              // 1. meet v-else, means the v-if/v-else-if chain is finished
              // 2. meet v-if while the v_if_manager is not empty, means the previous v-if/v-else-if chain is finished
              // 3. meet v-else/v-else-if with no v-if, v_if_manager won't add it to the chain, so add it to result there
              result.push(child);
            }
          } else {
            if let Some(chain) = v_if_manager.take_chain() {
              result.push(chain);
            }
            result.push(child);
          }
        }
        AstNode::Text(_) => {}
        AstNode::Comment(comment) => result.push(self.parse_comment(&comment)),
        AstNode::Interpolation(interp) => result.push(self.parse_interpolation(&interp)),
      }
    }

    if let Some(chain) = v_if_manager.take_chain() {
      // If the last element is v-if / v-else-if / v-else, push all the children
      result.push(chain);
    }
    result
  }

  pub fn parse_element(
    &mut self,
    node: Element<'a>,
    children: Option<ArenaVec<'a, JSXChild<'a>>>,
  ) -> (JSXChild<'a>, Option<VIf<'a>>) {
    let ast = self.ast;

    let open_element_span = {
      let start = node.location.start.offset;
      let tag_name_end = if let Some(prop) = node.properties.last() {
        match prop {
          ElemProp::Attr(prop) => prop.location.end.offset,
          ElemProp::Dir(prop) => prop.location.end.offset,
        }
      } else {
        start + 1 /* < */ + node.tag_name.len()
      };
      let end = memchr::memchr(b'>', &self.source_text.as_bytes()[tag_name_end..])
        .map(|i| tag_name_end + i + 1)
        .unwrap(); // SAFETY: The tag must be closed. Or parser will treat it as panicked.
      Span::new(start as u32, end as u32)
    };

    let location_span = node.location.span();
    let tag_name = node.tag_name;
    let end_element_span = {
      if location_span.source_text(self.source_text).ends_with("/>") || is_void_tag!(tag_name) {
        node.location.span()
      } else {
        let end = node.location.end.offset;
        let start =
          memchr::memrchr(b'<', &self.source_text.as_bytes()[..end]).map(|i| i as u32).unwrap();
        Span::new(start, end as u32)
      }
    };

    // Use different JSXElementName for component and normal element
    let allocator = Allocator::new();
    let mut element_name = {
      let name_span = Span::sized(open_element_span.start + 1, node.tag_name.len() as u32);

      if tag_name.contains('.')
        && let Some(expr) = unsafe {
          let original_source_type = self.source_type; // source_type implemented [`Copy`] trait
          self.source_type = self.source_type.with_jsx(true);

          // Directly call oxc_parser because it's too complex to process <a.b.c.d.e />
          // SAFETY: use `()` as wrap
          let expr = self.parse_expression(name_span, b"(<", b"/>)", &allocator);

          self.source_type = original_source_type;

          expr
        }
        && let Expression::JSXElement(mut jsx_element) = expr
      {
        // For namespace tag name, e.g. <motion.div />
        jsx_element.opening_element.name.take_in(self.allocator)
      } else if tag_name.contains('-') || tag_name == "component" {
        // For <keep-alive />
        let name = kebab_to_case(tag_name, true);
        ast.jsx_element_name_identifier_reference(name_span, ast.str(&name))
      } else {
        let name = ast.str(node.tag_name);
        if node.is_component() {
          // For <KeepAlive />
          ast.jsx_element_name_identifier_reference(name_span, name)
        } else {
          // For normal element, like <div>, use identifier
          ast.jsx_element_name_identifier(name_span, name)
        }
      }
    }
    .clone_in(self.allocator);

    let mut v_for_wrapper = VForWrapper::new(&ast);
    let mut v_slot_wrapper = VSlotWrapper::new(&ast);
    let mut v_if_state: Option<VIf<'a>> = None;
    let mut attributes = ast.vec();
    for prop in node.properties {
      attributes.push(self.parse_prop(
        prop,
        &mut v_for_wrapper,
        &mut v_slot_wrapper,
        &mut v_if_state,
      ));
    }

    let children = match children {
      Some(children) => children,
      None => v_slot_wrapper.wrap(self.parse_children(
        open_element_span.end,
        end_element_span.start,
        node.children,
      )),
    };

    // Clone element_name for opening element (needed because we may consume it in closing element)
    let opening_element_name = element_name.clone_in(self.allocator);

    // Determine closing element based on tag type:
    // - Self-closing tags (/>): closing element with empty name
    // - Void tags without />: None
    // - Normal tags with </tag>: closing element with tag name
    // Always use </tag> in codegen mode (prevent tag-hoist in v-slot children)
    let closing_element = if location_span.source_text(self.source_text).ends_with("/>") {
      if self.config.codegen {
        Some(ast.jsx_closing_element(SPAN, element_name.clone_in(self.allocator)))
      } else {
        Some(ast.jsx_closing_element(SPAN, ast.jsx_element_name_identifier(SPAN, ast.str(""))))
      }
    } else if is_void_tag!(tag_name) {
      if self.config.codegen {
        Some(ast.jsx_closing_element(SPAN, element_name.clone_in(self.allocator)))
      } else {
        None
      }
    } else {
      // Normal tag with explicit closing tag
      Some(ast.jsx_closing_element(end_element_span, {
        let span = Span::sized(end_element_span.start + 2, node.tag_name.len() as u32);
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
    prop: ElemProp<'a>,
    v_for_wrapper: &mut VForWrapper<'_, 'a>,
    v_slot_wrapper: &mut VSlotWrapper<'_, 'a>,
    v_if_state: &mut Option<VIf<'a>>,
  ) -> JSXAttributeItem<'a> {
    let ast = self.ast;
    match prop {
      // For normal attributes, like <div class="w-100" />
      ElemProp::Attr(attr) => {
        let attr_end = self.roffset(attr.location.end.offset) as u32;
        let attr_span = Span::new(attr.location.start.offset as u32, attr_end);
        ast.jsx_attribute_item_attribute(
          attr_span,
          ast.jsx_attribute_name_identifier(attr.name_loc.span(), ast.str(attr.name)),
          if let Some(value) = attr.value {
            Some(ast.jsx_attribute_value_string_literal(
              Span::new(value.location.span().start + 1, attr_end - 1),
              ast.str(value.content.raw),
              None,
            ))
          } else {
            None
          },
        )
      }
      // Directive, starts with `v-`
      ElemProp::Dir(dir) => {
        let dir_start = dir.location.start.offset as u32;
        let dir_end = self.roffset(dir.location.end.offset) as u32;

        let dir_name = self.parse_directive_name(&dir);
        // Analyze v-slot and v-for, no matter whether there is an expression
        if dir.name == "slot" {
          self.analyze_v_slot(&dir, v_slot_wrapper, &dir_name);
        } else if dir.name == "for" {
          self.analyze_v_for(&dir, v_for_wrapper);
        } else if dir.name == "else" {
          // v-else can have no expression
          *v_if_state = Some(VIf::Else);
        }

        if matches!(dir.name, "if" | "else-if") && dir.has_empty_expr() {
          error::v_if_else_without_expression(&mut self.errors, dir.location.span());
        }

        // This branch won't return `a=b` attribute but a `...x` struct
        // So we picked this logic into a separate block instead of a elseif branch in the under if-else chain
        if dir.name == "bind"
          && dir.argument.is_none()
          && let Some(expr_node) = &dir.expression
          && let Some(argument) = self.parse_pure_expression(Span::new(
            (expr_node.location.start.offset + 1) as u32,
            dir_end - 1,
          ))
        {
          // v-bind="expr" or :="expr" without an argument → JSX spread attribute {...expr}.
          // Vue treats argument-less v-bind as an object spread onto the element, which maps
          // directly to JSX spread: <div v-bind="obj" /> ↔ <div {...obj} />.
          // https://play.vuejs.org/#eNqVkbtOwzAUhl/FOkuWNC2CKQqVAFWiDICA0UuID8HFsS1f0khR3h3bVS9DVamb/V/s7+iM8KB10XuEEiqHnRa1wyWVhFSM96SfffPJ7imMhLOSZLXWWU4aUVsbbtvZzWKRkYnCkjyvSTUPlWO3vLJWzU/+hxycbZT84W2xsUoGvDG+TKFRneYCzZt2XElLoSTJiV4thNq+JM0Zj/leb36x+Tujb+wQNQrvBi2aHikcPFebFt3OXn2+4hDOB7NTzIuQvmB+oFXCR8Zd7NFLFrBPcol23WllHJftl10NDqXdDxVBY3JKeQphR08XRj/i3hZ3qUflBNM/rC6XVg==
          return ast.jsx_attribute_item_spread_attribute(Span::new(dir_start, dir_end), argument);
        }

        let value = if let Some(expr) = &dir.expression {
          // +1 to skip the opening quote
          let expr_start = expr.location.start.offset + 1;
          Some(
            ast.jsx_attribute_value_expression_container(
              Span::new(expr.location.span().start, dir_end),
              ((|| {
                // Use placeholder for v-for and v-slot
                if matches!(dir.name, "for" | "slot" | "else") {
                  None
                } else {
                  let expr = self.parse_pure_expression(Span::new(expr_start as u32, dir_end - 1));
                  if dir.name == "if" {
                    *v_if_state = expr.map(VIf::If);
                    None
                  } else if dir.name == "else-if" {
                    *v_if_state = expr.map(VIf::ElseIf);
                    None
                  } else {
                    // For possible dynamic arguments
                    Some(JSXExpression::from(self.parse_dynamic_argument(&dir, expr?)?))
                  }
                }
              })())
              .unwrap_or_else(|| self.empty_jsx_attribute_expression()),
            ),
          )
        } else if let Some(argument) = &dir.argument
          && let DirectiveArg::Dynamic(_) = argument
          && let Some(argument) =
            self.parse_dynamic_argument(&dir, ast.expression_identifier(SPAN, "undefined"))
        {
          // v-slot:[name]
          Some(ast.jsx_attribute_value_expression_container(SPAN, argument.into()))
        } else if dir.name == "bind"
          && let Some(argument) = dir.argument
          && let DirectiveArg::Static(arg_name) = argument
        {
          // :prop without value -> synthesize :prop="prop" (identifier reference).
          // Vue normalizes dashed prop names to camelCase (:msg-id -> msgId).
          // https://play.vuejs.org/#eNp9kUFLxDAQhf/KmEsV1pZFT6UuqCy4HlRU8JJLaadt1jQJSboWSv+7k5Zde5C9ZeZ98/ImGdi9MfGhQ5ayzBVWGA8OfWc2XInWaOthAIsVjFBZ3UJEaMQVV4VWzkPr6l0Jd4G4jJ5QSg1f2sryIrriKktmQ7KiwmNrZO6RKoCsWUNKw9ei3MBiLkuaNQFZsqDZinlH11WijvdOK0o6BA/OCt0aIdG+Gi8oDmcpTErQcvL8eZ563na4OvaLBovvf/p714ceZ28WHdoDcnbSfG5r9LO8/XjBns4nsdVlJ4k+I76j07ILGWfsoVMlxV5wU9rd9N5C1Z9u23tU7rhUCBrIceI5oz94PLP6X9yb+Haa42pk4y+ZtaHr
          let ident_name = kebab_to_case(arg_name, false);
          let ident_str = ast.str(&ident_name);
          Some(ast.jsx_attribute_value_expression_container(
            SPAN,
            JSXExpression::from(ast.expression_identifier(SPAN, ident_str)),
          ))
        } else {
          None
        };

        ast.jsx_attribute_item_attribute(
          Span::new(dir_start, dir_end),
          // Attribute Name
          dir_name,
          // Attribute Value
          value,
        )
      }
    }
  }

  fn parse_dynamic_argument(
    &mut self,
    dir: &Directive<'a>,
    expression: Expression<'a>,
  ) -> Option<Expression<'a>> {
    let head_name = dir.head_loc.span().source_text(self.source_text);
    let dir_start = dir.location.start.offset;
    if let Some(argument) = &dir.argument
      && let DirectiveArg::Dynamic(argument_str) = argument
    {
      let dynamic_arg_expression = self.parse_pure_expression({
        Span::sized(
          if head_name.starts_with("v-") {
            dir_start + 2 + dir.name.len() + 2 // v-bind:[arg] -> skip `:[` (2 chars)
          } else {
            dir_start + 2 // :[arg] -> skip `:[` (2 chars)
          } as u32,
          argument_str.len() as u32,
        )
      })?;

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

  fn empty_jsx_attribute_expression(&self) -> JSXExpression<'a> {
    if self.config.codegen {
      JSXExpression::from(self.ast.expression_identifier(SPAN, "undefined"))
    } else {
      self.ast.jsx_expression_empty_expression(SPAN)
    }
  }

  fn parse_comment(&mut self, comment: &SourceNode<'a>) -> JSXChild<'a> {
    let ast = self.ast;
    let span = comment.location.span();
    let start = comment.source.as_ptr() as usize - self.source_text.as_ptr() as usize;
    let end = start + comment.source.len();
    self.comments.push(Comment::new(
      start as u32,
      end as u32,
      if comment.source.contains('\n') {
        CommentKind::MultiLineBlock
      } else {
        CommentKind::SingleLineBlock
      },
    ));
    ast.jsx_child_expression_container(span, ast.jsx_expression_empty_expression(SPAN))
  }

  fn parse_interpolation(&mut self, introp: &SourceNode<'a>) -> JSXChild<'a> {
    let ast = self.ast;
    // Use full span for container (includes {{ and }})
    let container_span = introp.location.span();
    // Expression starts after {{ (2 characters)
    let expr_start = introp.location.start.offset + 2;

    ast.jsx_child_expression_container(
      container_span,
      self
        .parse_pure_expression(Span::new(
          expr_start as u32,
          (expr_start + introp.source.len()) as u32,
        ))
        .map_or_else(|| ast.jsx_expression_empty_expression(SPAN), JSXExpression::from),
    )
  }

  pub fn parse_pure_expression(&mut self, span: Span) -> Option<Expression<'a>> {
    let allocator = Allocator::new();
    // SAFETY: use `()` as wrap
    unsafe { self.parse_expression(span, b"(", b")", &allocator).clone_in(self.allocator) }
  }

  /// Parse expression with [`oxc_parser`]
  /// The reason we don't wrap the expression with `(` and `)` is to avoid unnecessary copy
  /// `b"(("` and `b")=>{})"` is much more efficient than passing `b"("` `b")=>{}"` and copy it in a [`Vec`] and push and slice
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
    // The only purpose to not use [`oxc_parser::Parser::parse_expression`] is to keep the code comments in it
    let (_, mut body, _) = self.oxc_parse(span, start_wrap, end_wrap, Some(allocator))?;

    let Some(Statement::ExpressionStatement(stmt)) = body.get_mut(0) else {
      // SAFETY: We always wrap the source in parentheses, so it should always be an expression statement.
      unreachable!()
    };
    let Expression::ParenthesizedExpression(expression) = &mut stmt.expression else {
      // SAFETY: We always wrap the source in parentheses, so it should always be a parenthesized expression
      unreachable!()
    };
    Some(expression.expression.take_in(self.allocator))
  }

  fn roffset(&self, end: usize) -> usize {
    end - self.source_text[..end].chars().rev().take_while(|c| c.is_whitespace()).count()
  }
}
