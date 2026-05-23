use oxc_allocator::{Box as ArenaBox, Vec as ArenaVec};
use oxc_ast::ast::{Directive, Expression, FormalParameters, Statement};
use oxc_span::Span;

use crate::VueParser;
use crate::ast::{
  VAttribute, VDirective, VDirectiveArgument, VDirectiveArgumentExpression, VDirectiveExpression,
  VDirectiveKey, VElement, VEndTag, VForDirective, VForExpression, VIdentifier, VInterpolation,
  VLiteral, VNode, VPureAttribute, VPureScript, VSlotDirective, VSlotExpression, VStartTag, VText,
  Variable,
  bindings::{
    ReferenceKind, collect_expression_references, collect_expression_references_with_kind,
    collect_parameter_variables,
  },
};

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
const SVG_NS: &str = "http://www.w3.org/2000/svg";
const MATH_NS: &str = "http://www.w3.org/1998/Math/MathML";

#[derive(Debug)]
pub(super) enum ParsedNode<'b> {
  Element(ParsedElement<'b>),
  Text(ParsedText<'b>),
  Interpolation(ParsedInterpolation<'b>),
  PureScript(ParsedPureScript<'b>),
}

#[derive(Debug)]
pub(super) struct ParsedElement<'b> {
  pub(super) name: &'b str,
  pub(super) raw_name: &'b str,
  pub(super) start_tag: ParsedStartTag<'b>,
  pub(super) children: Vec<ParsedNode<'b>>,
  pub(super) end_tag: Option<Span>,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedStartTag<'b> {
  pub(super) attributes: Vec<ParsedAttribute<'b>>,
  pub(super) self_closing: bool,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedText<'b> {
  pub(super) value: &'b str,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedInterpolation<'b> {
  pub(super) expression: Expression<'b>,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedPureScript<'b> {
  pub(super) directives: ArenaVec<'b, Directive<'b>>,
  pub(super) statements: ArenaVec<'b, Statement<'b>>,
  pub(super) setup: bool,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) enum ParsedAttribute<'b> {
  Pure(ParsedPureAttribute<'b>),
  Directive(ParsedDirective<'b>),
  Slot(ParsedSlotDirective<'b>),
  For(ParsedForDirective<'b>),
}

#[derive(Debug)]
pub(super) struct ParsedPureAttribute<'b> {
  pub(super) key: ParsedIdentifier<'b>,
  pub(super) value: Option<ParsedLiteral<'b>>,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedDirective<'b> {
  pub(super) key: ParsedDirectiveKey<'b>,
  pub(super) value: Option<ParsedDirectiveExpression<'b>>,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedSlotDirective<'b> {
  pub(super) key: ParsedDirectiveKey<'b>,
  pub(super) value: Option<ParsedSlotExpression<'b>>,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedForDirective<'b> {
  pub(super) key: ParsedDirectiveKey<'b>,
  pub(super) value: Option<ParsedForExpression<'b>>,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedDirectiveKey<'b> {
  pub(super) name: ParsedIdentifier<'b>,
  pub(super) argument: Option<ParsedDirectiveArgument<'b>>,
  pub(super) modifiers: Vec<ParsedIdentifier<'b>>,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) enum ParsedDirectiveArgument<'b> {
  Dynamic(ParsedDirectiveExpression<'b>),
  Static(ParsedIdentifier<'b>),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ParsedIdentifier<'b> {
  pub(super) name: &'b str,
  pub(super) raw_name: &'b str,
  pub(super) span: Span,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ParsedLiteral<'b> {
  pub(super) value: &'b str,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedDirectiveExpression<'b> {
  pub(super) expression: Expression<'b>,
  pub(super) reference_kind: ReferenceKind,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedSlotExpression<'b> {
  pub(super) params: ArenaBox<'b, FormalParameters<'b>>,
  pub(super) expression_span: Span,
  pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ParsedForExpression<'b> {
  pub(super) left: ArenaBox<'b, FormalParameters<'b>>,
  pub(super) right: Expression<'b>,
  pub(super) expression_span: Span,
  pub(super) span: Span,
}

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub(super) fn push_parsed_node(
    root: &mut Vec<ParsedNode<'b>>,
    stack: &mut [ParsedElement<'b>],
    node: ParsedNode<'b>,
  ) {
    if let Some(parent) = stack.last_mut() {
      parent.children.push(node);
    } else {
      root.push(node);
    }
  }

  pub(super) fn build_arena_nodes(
    &self,
    nodes: Vec<ParsedNode<'b>>,
  ) -> ArenaVec<'a, VNode<'a, 'b>> {
    self.build_arena_nodes_in_namespace(nodes, HTML_NS)
  }

  fn build_arena_nodes_in_namespace(
    &self,
    nodes: Vec<ParsedNode<'b>>,
    namespace: &'static str,
  ) -> ArenaVec<'a, VNode<'a, 'b>> {
    let mut arena_nodes = ArenaVec::new_in(self.vue_allocator);
    for node in nodes {
      arena_nodes.push(self.build_arena_node(node, namespace));
    }
    arena_nodes
  }

  pub(super) fn build_arena_node(
    &self,
    node: ParsedNode<'b>,
    namespace: &'static str,
  ) -> VNode<'a, 'b> {
    match node {
      ParsedNode::Element(element) => {
        VNode::Element(ArenaBox::new_in(
          self.build_arena_element(element, namespace),
          self.vue_allocator,
        ))
      }
      ParsedNode::Text(text) => VNode::Text(ArenaBox::new_in(
        VText { text: text.value, span: text.span },
        self.vue_allocator,
      )),
      ParsedNode::Interpolation(interpolation) => VNode::Interpolation(ArenaBox::new_in(
        VInterpolation {
          references: collect_expression_references(self.vue_allocator, &interpolation.expression),
          expression: interpolation.expression,
          span: interpolation.span,
        },
        self.vue_allocator,
      )),
      ParsedNode::PureScript(script) => VNode::PureScript(ArenaBox::new_in(
        VPureScript {
          directives: script.directives,
          statements: script.statements,
          setup: script.setup,
          span: script.span,
        },
        self.vue_allocator,
      )),
    }
  }

  fn build_arena_element(
    &self,
    element: ParsedElement<'b>,
    namespace: &'static str,
  ) -> VElement<'a, 'b> {
    let child_namespace = element_namespace(element.name, namespace);
    let variables = self.collect_element_variables(&element.start_tag.attributes);
    let mut attributes = ArenaVec::new_in(self.vue_allocator);
    for attribute in element.start_tag.attributes {
      attributes.push(self.build_arena_attribute(attribute));
    }

    VElement {
      name: element.name,
      raw_name: element.raw_name,
      namespace: child_namespace,
      start_tag: VStartTag {
        attributes,
        self_closing: element.start_tag.self_closing,
        span: element.start_tag.span,
      },
      children: self.build_arena_nodes_in_namespace(element.children, child_namespace),
      end_tag: element.end_tag.map(|span| VEndTag { span }),
      variables,
      style: element.name == "style",
      span: element.span,
    }
  }

  fn collect_element_variables(
    &self,
    attributes: &[ParsedAttribute<'b>],
  ) -> ArenaVec<'a, Variable<'b>> {
    let mut variables = ArenaVec::new_in(self.vue_allocator);
    for attribute in attributes {
      match attribute {
        ParsedAttribute::Slot(attribute) => {
          if let Some(value) = &attribute.value {
            variables.extend(collect_parameter_variables(
              self.vue_allocator,
              &value.params,
              "scope",
            ));
          }
        }
        ParsedAttribute::For(attribute) => {
          if let Some(value) = &attribute.value {
            variables.extend(collect_parameter_variables(self.vue_allocator, &value.left, "v-for"));
          }
        }
        ParsedAttribute::Pure(_) | ParsedAttribute::Directive(_) => {}
      }
    }
    variables
  }

  fn build_arena_attribute(&self, attribute: ParsedAttribute<'b>) -> VAttribute<'a, 'b> {
    match attribute {
      ParsedAttribute::Pure(attribute) => VAttribute::VPureAttribute(ArenaBox::new_in(
        VPureAttribute {
          key: Self::build_identifier(attribute.key),
          value: attribute.value.map(Self::build_literal),
          span: attribute.span,
        },
        self.vue_allocator,
      )),
      ParsedAttribute::Directive(attribute) => VAttribute::VDirective(ArenaBox::new_in(
        VDirective {
          key: self.build_directive_key(attribute.key),
          value: attribute.value.map(|value| VDirectiveExpression {
            references: collect_expression_references_with_kind(
              self.vue_allocator,
              &value.expression,
              value.reference_kind,
            ),
            expression: value.expression,
            span: value.span,
          }),
          span: attribute.span,
        },
        self.vue_allocator,
      )),
      ParsedAttribute::Slot(attribute) => VAttribute::VSlotDirective(ArenaBox::new_in(
        VSlotDirective {
          key: self.build_directive_key(attribute.key),
          value: attribute.value.map(|value| VSlotExpression {
            params: value.params,
            expression_span: value.expression_span,
            span: value.span,
          }),
          span: attribute.span,
        },
        self.vue_allocator,
      )),
      ParsedAttribute::For(attribute) => VAttribute::VForDirective(ArenaBox::new_in(
        VForDirective {
          key: self.build_directive_key(attribute.key),
          value: attribute.value.map(|value| VForExpression {
            references: collect_expression_references(self.js_allocator, &value.right),
            left: value.left,
            right: value.right,
            expression_span: value.expression_span,
            span: value.span,
          }),
          span: attribute.span,
        },
        self.vue_allocator,
      )),
    }
  }

  fn build_directive_key(&self, key: ParsedDirectiveKey<'b>) -> VDirectiveKey<'a, 'b> {
    let name = self.vue_allocator.alloc(Self::build_identifier(key.name));

    let argument = match key.argument {
      Some(ParsedDirectiveArgument::Dynamic(value)) => {
        Some(VDirectiveArgument::VDirectiveArgument(ArenaBox::new_in(
          VDirectiveArgumentExpression {
            references: collect_expression_references_with_kind(
              self.vue_allocator,
              &value.expression,
              value.reference_kind,
            ),
            expression: value.expression,
            span: value.span,
          },
          self.vue_allocator,
        )))
      }
      Some(ParsedDirectiveArgument::Static(identifier)) => Some(VDirectiveArgument::VIdentifier(
        ArenaBox::new_in(Self::build_identifier(identifier), self.vue_allocator),
      )),
      None => None,
    };

    let mut modifiers = ArenaVec::new_in(self.vue_allocator);
    for modifier in key.modifiers {
      modifiers.push(Self::build_identifier(modifier));
    }

    VDirectiveKey { name, argument, modifiers, span: key.span }
  }

  const fn build_identifier(identifier: ParsedIdentifier<'b>) -> VIdentifier<'a> {
    VIdentifier { name: identifier.name, raw_name: identifier.raw_name, span: identifier.span }
  }

  const fn build_literal(literal: ParsedLiteral<'b>) -> VLiteral<'a> {
    VLiteral { value: literal.value, span: literal.span }
  }
}

const fn element_namespace(name: &str, current: &'static str) -> &'static str {
  match name.as_bytes() {
    b"svg" => SVG_NS,
    b"math" => MATH_NS,
    _ => current,
  }
}
