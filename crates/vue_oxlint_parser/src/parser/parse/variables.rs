use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{BindingPattern, FormalParameters};

use crate::ast::{VAttribute, Variable};
use crate::parser::parse::TemplateParser;

impl<'a, 'b> TemplateParser<'_, 'a, 'b>
where
  'b: 'a,
{
  pub(super) fn collect_start_tag_variables(
    &self,
    attributes: &ArenaVec<'a, VAttribute<'a, 'b>>,
  ) -> ArenaVec<'a, Variable<'a>> {
    let mut variables = ArenaVec::new_in(self.parser.vue_allocator);

    for attribute in attributes {
      match attribute {
        VAttribute::VForDirective(directive) => {
          self.collect_formal_parameter_variables(&directive.value.left, "v-for", &mut variables);
        }
        VAttribute::VSlotDirective(directive) => {
          self.collect_formal_parameter_variables(
            &directive.value.params,
            "v-slot",
            &mut variables,
          );
        }
        VAttribute::VPureAttribute(_) | VAttribute::VDirective(_) | VAttribute::VOnDirective(_) => {
        }
      }
    }

    variables
  }

  pub(super) fn clone_variables(
    &self,
    variables: &ArenaVec<'a, Variable<'a>>,
  ) -> ArenaVec<'a, Variable<'a>> {
    let mut cloned = ArenaVec::new_in(self.parser.vue_allocator);
    for variable in variables {
      cloned.push(Variable { name: variable.name, span: variable.span, kind: variable.kind });
    }
    cloned
  }

  fn collect_formal_parameter_variables(
    &self,
    params: &FormalParameters<'b>,
    kind: &'static str,
    variables: &mut ArenaVec<'a, Variable<'a>>,
  ) {
    for param in &params.items {
      self.collect_binding_pattern_variables(&param.pattern, kind, variables);
    }
    if let Some(rest) = &params.rest {
      self.collect_binding_pattern_variables(&rest.rest.argument, kind, variables);
    }
  }

  fn collect_binding_pattern_variables(
    &self,
    pattern: &BindingPattern<'b>,
    kind: &'static str,
    variables: &mut ArenaVec<'a, Variable<'a>>,
  ) {
    match pattern {
      BindingPattern::BindingIdentifier(identifier) => {
        variables.push(Variable {
          name: self.parser.vue_allocator.alloc_str(identifier.name.as_str()),
          span: identifier.span,
          kind,
        });
      }
      BindingPattern::ObjectPattern(pattern) => {
        for property in &pattern.properties {
          self.collect_binding_pattern_variables(&property.value, kind, variables);
        }
        if let Some(rest) = &pattern.rest {
          self.collect_binding_pattern_variables(&rest.argument, kind, variables);
        }
      }
      BindingPattern::ArrayPattern(pattern) => {
        for element in pattern.elements.iter().flatten() {
          self.collect_binding_pattern_variables(element, kind, variables);
        }
        if let Some(rest) = &pattern.rest {
          self.collect_binding_pattern_variables(&rest.argument, kind, variables);
        }
      }
      BindingPattern::AssignmentPattern(pattern) => {
        self.collect_binding_pattern_variables(&pattern.left, kind, variables);
      }
    }
  }
}
