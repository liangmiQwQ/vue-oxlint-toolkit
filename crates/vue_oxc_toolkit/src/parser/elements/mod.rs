use oxc_allocator::{Allocator, CloneIn, TakeIn, Vec as ArenaVec};
use oxc_ast::{
  Comment, CommentKind, NONE,
  ast::{Expression, JSXAttributeItem, JSXChild, JSXExpression, PropertyKind, Statement},
};
use oxc_span::{GetSpanMut, SPAN, Span};
use vize_armature::{
  CommentNode, DirectiveNode, ElementNode, ElementType, InterpolationNode, PropNode,
  TemplateChildNode, TextNode,
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

/// Convert kebab-case to camel-like case.
/// `pascal: true` -> `PascalCase` (e.g. `keep-alive` -> `KeepAlive`)
/// `pascal: false` -> `camelCase`  (e.g. `msg-id` -> `msgId`)
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
    start: u32,
    end: u32,
    children: &[TemplateChildNode<'_>],
  ) -> ArenaVec<'a, JSXChild<'a>> {
    let ast = self.ast;
    if children.is_empty() {
      return ast.vec();
    }
    let mut result = self.ast.vec_with_capacity(children.len() + 2);

    // Track position after the last element/interpolation to synthesize gap text nodes.
    // We use `None` until the first element/interpolation is encountered.
    // Text nodes from vize are NOT used for gaps; we create them from source spans instead.
    // Comments do NOT advance last_elem_end (gaps are only around elements/interpolations).
    let mut last_elem_end: Option<u32> = None;

    let mut v_if_manager = VIfManager::new(&ast);
    for child in children {
      match child {
        TemplateChildNode::Element(node) => {
          // Synthesize gap text between last element/interpolation and this one
          let gap_from = last_elem_end.unwrap_or(start);
          let child_start = node.loc.start.offset;
          if child_start > gap_from {
            let span = Span::new(gap_from, child_start);
            let value = ast.str(span.source_text(self.source_text));
            result.push(ast.jsx_child_text(span, value, Some(value)));
          }

          let (child, v_if) = self.parse_element_ref(node, None);

          // Advance last_elem_end to true element end (vize loc only covers opening tag)
          let tag = node.tag.as_str();
          last_elem_end = Some(if node.is_self_closing || is_void_tag!(tag) {
            node.loc.end.offset
          } else {
            self.element_close_span(node.loc.end.offset, tag).end
          });

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
        TemplateChildNode::Text(text) => {
          let new_child = self.parse_text(text);
          // Merge with previous JSXText if adjacent (vize splits text at '<' chars)
          if let Some(JSXChild::Text(prev)) = result.last_mut()
            && let JSXChild::Text(cur) = &new_child
            && prev.span.end == cur.span.start
          {
            let merged_span = Span::new(prev.span.start, cur.span.end);
            let merged_val = ast.str(merged_span.source_text(self.source_text));
            prev.span = merged_span;
            prev.value = merged_val;
            prev.raw = Some(merged_val);
          } else {
            result.push(new_child);
          }
        }
        TemplateChildNode::Comment(comment) => result.push(self.parse_comment(comment)),
        TemplateChildNode::Interpolation(interp) => {
          // Same gap logic as Element
          let gap_from = last_elem_end.unwrap_or(start);
          let child_start = interp.loc.start.offset;
          if child_start > gap_from {
            let span = Span::new(gap_from, child_start);
            let value = ast.str(span.source_text(self.source_text));
            result.push(ast.jsx_child_text(span, value, Some(value)));
          }
          last_elem_end = Some(interp.loc.end.offset);
          result.push(self.parse_interpolation(interp));
        }
        _ => {
          // Other node types (If, For, TextCall, etc.) should not appear at parse stage
        }
      }
    }

    if let Some(chain) = v_if_manager.take_chain() {
      result.push(chain);
    }

    // Trailing gap after last element/interpolation (only if we saw at least one)
    if let Some(elem_end) = last_elem_end
      && elem_end < end
    {
      let span = Span::new(elem_end, end);
      let value = ast.str(span.source_text(self.source_text));
      result.push(ast.jsx_child_text(span, value, Some(value)));
    }

    result
  }

  pub fn parse_element_ref(
    &mut self,
    node: &ElementNode<'_>,
    children: Option<ArenaVec<'a, JSXChild<'a>>>,
  ) -> (JSXChild<'a>, Option<VIf<'a>>) {
    let ast = self.ast;

    let tag_src = node.loc.span().source_text(self.source_text);
    // Extract just the tag name from the source (between < and first whitespace or >)
    let tag_name_str = tag_src[1..] // skip '<'
      .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
      .next()
      .unwrap_or("");

    let open_element_span = {
      let start = node.loc.start.offset;
      let tag_name_end = node
        .props
        .last()
        .map_or_else(|| start + 1 + tag_name_str.len() as u32, |prop| prop.loc().end.offset);

      let end = memchr::memchr(b'>', &self.source_text.as_bytes()[tag_name_end as usize..])
        .map(|i| tag_name_end + i as u32 + 1)
        .unwrap(); // SAFETY: The tag must be closed. Or parser will treat it as panicked.
      Span::new(start, end)
    };

    // Vize's node.loc only covers the opening tag. Scan forward to find the closing tag.
    let (location_span, end_element_span) = if node.is_self_closing || is_void_tag!(tag_name_str) {
      (node.loc.span(), node.loc.span())
    } else {
      let close_span = self.element_close_span(node.loc.end.offset, tag_name_str);
      let full_span = Span::new(node.loc.start.offset, close_span.end);
      (full_span, close_span)
    };

    // Use different JSXElementName for component and normal element
    let allocator = Allocator::new();
    let mut element_name = {
      let name_span = Span::sized(open_element_span.start + 1, tag_name_str.len() as u32);

      if tag_name_str.contains('.')
        && let Some(expr) = unsafe {
          let original_source_type = self.source_type;
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
      } else if tag_name_str.contains('-') {
        // For <keep-alive />
        let name = kebab_to_case(tag_name_str, true);
        ast.jsx_element_name_identifier_reference(name_span, ast.str(&name))
      } else {
        let name = ast.str(tag_name_str);
        // <component> is Vue's built-in dynamic component; treat it as a component reference
        // even though vize doesn't classify it as ElementType::Component.
        if node.tag_type == ElementType::Component || tag_name_str == "component" {
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
    } else if is_void_tag!(tag_name_str) {
      None
    } else {
      Some(ast.jsx_closing_element(end_element_span, {
        let span = Span::sized(end_element_span.start + 2, tag_name_str.len() as u32);
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
      // For normal attributes, like <div class="w-100" />
      PropNode::Attribute(attr) => {
        // vize attr.loc.end points AT the closing quote char (not past it) for quoted values
        let loc_end = attr.loc.end.offset as usize;
        let attr_end = if attr.value.is_some()
          && matches!(self.source_text.as_bytes().get(loc_end), Some(&(b'"' | b'\'')))
        {
          (loc_end + 1) as u32
        } else {
          self.roffset(loc_end) as u32
        };
        let attr_span = Span::new(attr.loc.start.offset, attr_end);
        ast.jsx_attribute_item_attribute(
          attr_span,
          ast.jsx_attribute_name_identifier(attr.name_loc.span(), {
            let name_text = attr.name_loc.span().source_text(self.source_text);
            ast.str(name_text)
          }),
          if let Some(value) = &attr.value {
            // vize TextNode.loc doesn't include quotes, so use it directly for content
            let value_span = value.loc.span();
            Some(ast.jsx_attribute_value_string_literal(
              value_span,
              ast.str(value_span.source_text(self.source_text)),
              None,
            ))
          } else {
            None
          },
        )
      }
      // Directive, starts with `v-`
      PropNode::Directive(dir) => {
        let dir_start = dir.loc.start.offset;
        // vize dir.loc.end points AT the closing quote char (not past it); adjust
        let dir_loc_end = dir.loc.end.offset as usize;
        let dir_span = self.directive_span(dir);
        let dir_end = dir_span.end;

        let dir_name = self.parse_directive_name(dir);
        // Analyze v-slot and v-for, no matter whether there is an expression
        if dir.name.as_str() == "slot" {
          self.analyze_v_slot(dir, v_slot_wrapper, &dir_name);
        } else if dir.name.as_str() == "for" {
          self.analyze_v_for(dir, v_for_wrapper);
        } else if dir.name.as_str() == "else" {
          // v-else can have no expression
          *v_if_state = Some(VIf::Else);
        }

        if matches!(dir.name.as_str(), "if" | "else-if") && dir.exp.is_none() {
          error::v_if_else_without_expression(&mut self.errors, dir_span);
        }

        let value = if let Some(exp) = &dir.exp {
          let exp_loc = exp.loc();
          // vize expression loc doesn't include quotes
          let expr_span = exp_loc.span();
          Some(
            ast.jsx_attribute_value_expression_container(
              // -1 to include the opening quote in the container span
              Span::new(expr_span.start.saturating_sub(1), dir_end),
              ((|| {
                // Use placeholder for v-for and v-slot
                if matches!(dir.name.as_str(), "for" | "slot" | "else") {
                  None
                } else {
                  let expr = self.parse_pure_expression(expr_span);
                  if dir.name.as_str() == "if" {
                    *v_if_state = expr.map(VIf::If);
                    None
                  } else if dir.name.as_str() == "else-if" {
                    *v_if_state = expr.map(VIf::ElseIf);
                    None
                  } else {
                    Some(JSXExpression::from(self.parse_dynamic_argument(dir, expr?)?))
                  }
                }
              })())
              .unwrap_or_else(|| JSXExpression::EmptyExpression(ast.jsx_empty_expression(SPAN))),
            ),
          )
        } else if let Some(arg) = &dir.arg
          && !is_static_arg(arg)
          && let Some(argument) =
            self.parse_dynamic_argument(dir, ast.expression_identifier(SPAN, "undefined"))
        {
          // v-slot:[name]
          Some(ast.jsx_attribute_value_expression_container(SPAN, argument.into()))
        } else if dir.name.as_str() == "bind"
          && let Some(arg) = &dir.arg
          && is_static_arg(arg)
        {
          // :prop without value -> synthesize :prop="prop" (identifier reference).
          // Vue normalizes dashed prop names to camelCase (:msg-id -> msgId).
          let raw_arg = arg.loc().span().source_text(self.source_text);
          let ident_name = kebab_to_case(raw_arg, false);
          let ident_str = ast.str(&ident_name);
          Some(ast.jsx_attribute_value_expression_container(
            SPAN,
            JSXExpression::from(ast.expression_identifier(SPAN, ident_str)),
          ))
        } else if dir_end > dir_loc_end as u32 {
          // Empty quoted value like v-for="" — create ExpressionContainer for `""`
          let container_span = Span::new(dir_end - 2, dir_end);
          Some(ast.jsx_attribute_value_expression_container(
            container_span,
            JSXExpression::EmptyExpression(ast.jsx_empty_expression(SPAN)),
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
    dir: &DirectiveNode<'_>,
    expression: Expression<'a>,
  ) -> Option<Expression<'a>> {
    let head_span = self.compute_head_span(dir);
    let head_name = head_span.source_text(self.source_text);
    let dir_start = dir.loc.start.offset;
    if let Some(arg) = &dir.arg
      && !is_static_arg(arg)
    {
      let arg_loc = arg.loc();
      let dynamic_arg_expression = self.parse_pure_expression({
        Span::sized(
          if head_name.starts_with("v-") {
            dir_start + 2 + dir.name.len() as u32 + 2 // v-bind:[arg] -> skip `:[` (2 chars)
          } else {
            dir_start + 2 // :[arg] -> skip `:[` (2 chars)
          },
          arg_loc.end.offset - arg_loc.start.offset,
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
    // vize InterpolationNode.content.loc() gives the expression span (without {{ }})
    let expr_span = introp.content.loc().span();

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

  /// Parse expression with [`oxc_parser`]
  /// The reason we don't wrap the expression with `(` and `)` is to avoid unnecessary copy.
  /// `b\"((\"` and `b\")=>{})\"` is more efficient than passing small wrappers and reassembling.
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
    // The only purpose to not use [`oxc_parser::Parser::parse_expression`] is to keep the code comments in it.
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

  fn roffset(&self, end: usize) -> usize {
    end - self.source_text[..end].chars().rev().take_while(|c| c.is_whitespace()).count()
  }

  /// Find the closing tag span for an element given its opening tag end offset.
  /// Scans forward tracking nesting depth to find the matching `</tagname>`.
  pub(super) fn element_close_span(&self, open_end: u32, tag_name: &str) -> Span {
    let src = self.source_text.as_bytes();
    let tag_bytes = tag_name.as_bytes();
    let mut pos = open_end as usize;
    let mut depth = 1usize;

    while pos < src.len() {
      let Some(rel) = memchr::memchr(b'<', &src[pos..]) else { break };
      pos += rel;
      let rest = &src[pos + 1..];

      if rest.first() == Some(&b'/') {
        // Potential closing tag
        let after_slash = &rest[1..];
        if after_slash.starts_with(tag_bytes) {
          let after_name = tag_bytes.len();
          let ch = after_slash.get(after_name).copied().unwrap_or(b'>');
          if ch == b'>' || ch == b' ' || ch == b'\n' || ch == b'\r' || ch == b'\t' {
            depth -= 1;
            if depth == 0 {
              let gt = memchr::memchr(b'>', &src[pos..]).unwrap();
              return Span::new(pos as u32, (pos + gt + 1) as u32);
            }
          }
        }
      } else {
        // Potential opening tag — increase depth for same tag name (handle nesting)
        if rest.starts_with(tag_bytes) {
          let after_name = tag_bytes.len();
          let ch = rest.get(after_name).copied().unwrap_or(0);
          if ch == b'>' || ch == b' ' || ch == b'\n' || ch == b'\r' || ch == b'\t' || ch == b'/' {
            depth += 1;
          }
        }
      }
      pos += 1;
    }

    // Fallback (malformed source)
    Span::new(open_end, open_end)
  }

  /// Compute the "head" span of a directive — the directive prefix + name + argument portion
  /// before the `=` sign or end of directive if no value.
  /// Compute the full directive span with adjusted end:
  /// vize `dir.loc.end` points AT the closing quote char — add 1 to include it.
  /// For directives without quotes, trim trailing whitespace via `roffset`.
  fn directive_span(&self, dir: &DirectiveNode<'_>) -> Span {
    let loc_end = dir.loc.end.offset as usize;
    let end = if matches!(self.source_text.as_bytes().get(loc_end), Some(&(b'"' | b'\''))) {
      (loc_end + 1) as u32
    } else {
      self.roffset(loc_end) as u32
    };
    Span::new(dir.loc.start.offset, end)
  }

  fn compute_head_span(&self, dir: &DirectiveNode<'_>) -> Span {
    let dir_text = dir.loc.span().source_text(self.source_text);
    let head_end = dir_text.find('=').map_or_else(
      || self.roffset(dir.loc.end.offset as usize) as u32,
      |i| dir.loc.start.offset + i as u32,
    );
    Span::new(dir.loc.start.offset, head_end)
  }
}

/// Check if a directive argument expression is static
fn is_static_arg(arg: &vize_armature::ExpressionNode<'_>) -> bool {
  match arg {
    vize_armature::ExpressionNode::Simple(s) => s.is_static,
    vize_armature::ExpressionNode::Compound(_) => false,
  }
}
