use oxc_allocator::{Allocator, CloneIn, TakeIn, Vec as ArenaVec};
use oxc_ast::{
  Comment, CommentKind, NONE,
  ast::{Expression, JSXAttributeItem, JSXChild, JSXExpression, PropertyKind, Statement},
};
use oxc_span::{GetSpanMut, SPAN, Span};
use vue_compiler_core::parser::{
  AstNode, Directive, DirectiveArg, ElemProp, Element, SourceNode, TextNode,
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
    parse::SourceLocatonSpan,
  },
};

mod directive;
mod v_for;
mod v_if;
mod v_slot;

/// Convert a kebab-case string to camelCase, matching Vue's prop-name normalization.
/// e.g. `msg-id` → `msgId`, `foo` → `foo`
fn kebab_to_camel(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let mut capitalize_next = false;
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
    start: u32,
    end: u32,
    children: Vec<AstNode<'a>>,
  ) -> ArenaVec<'a, JSXChild<'a>> {
    let ast = self.ast;
    if children.is_empty() {
      return ast.vec();
    }
    let mut result = self.ast.vec_with_capacity(children.len() + 2);

    // Process the whitespaces text there <div>____<br>_____</div>
    if let Some(first) = children.first()
      && matches!(first, AstNode::Element(_) | AstNode::Interpolation(_))
      && start != first.get_location().start.offset as u32
    {
      let span = Span::new(start, first.get_location().start.offset as u32);
      let value = span.source_text(self.source_text);
      result.push(ast.jsx_child_text(span, value, Some(ast.str(value))));
    }

    let last = if let Some(last) = children.last()
      && matches!(last, AstNode::Element(_) | AstNode::Interpolation(_))
      && end != last.get_location().end.offset as u32
    {
      let span = Span::new(last.get_location().end.offset as u32, end);
      let value = span.source_text(self.source_text);
      Some(ast.jsx_child_text(span, value, Some(ast.str(value))))
    } else {
      None
    };

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
        AstNode::Text(text) => result.push(self.parse_text(&text)),
        AstNode::Comment(comment) => result.push(self.parse_comment(&comment)),
        AstNode::Interpolation(interp) => result.push(self.parse_interpolation(&interp)),
      }
    }

    if let Some(chain) = v_if_manager.take_chain() {
      // If the last element is v-if / v-else-if / v-else, push all the children
      result.push(chain);
    }
    if let Some(last) = last {
      result.push(last);
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
      } else if tag_name.contains('-') {
        // For <keep-alive />
        let name = tag_name
          .split('-')
          .map(|s| {
            // SAFETY to use ascii and not check bytes length
            let mut bytes = s.as_bytes().to_vec();
            bytes[0] = bytes[0].to_ascii_uppercase();
            String::from_utf8(bytes).unwrap()
          })
          .collect::<String>();

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
    let closing_element = if location_span.source_text(self.source_text).ends_with("/>") {
      // Self-closing tag: create closing element with empty element name
      Some(ast.jsx_closing_element(SPAN, ast.jsx_element_name_identifier(SPAN, ast.str(""))))
    } else if is_void_tag!(tag_name) {
      // Void tag without />: no closing element
      None
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
              .unwrap_or_else(|| JSXExpression::EmptyExpression(ast.jsx_empty_expression(SPAN))),
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
          // :prop without value → synthesize :prop="prop" (identifier reference).
          // Vue normalizes dashed prop names to camelCase (:msg-id → msgId).
          let ident_name = kebab_to_camel(arg_name);
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

  fn parse_text(&self, text: &TextNode<'a>) -> JSXChild<'a> {
    let raw = self.ast.str(&text.text.iter().map(|t| t.raw).collect::<String>());
    self.ast.jsx_child_text(text.location.span(), raw, Some(raw))
  }

  fn parse_comment(&mut self, comment: &SourceNode<'a>) -> JSXChild<'a> {
    let ast = self.ast;
    let span = comment.location.span();
    self.comments.push(Comment::new(
      span.start + 1,
      span.end - 1,
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
