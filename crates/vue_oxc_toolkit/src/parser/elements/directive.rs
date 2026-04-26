use memchr::memchr;
use oxc_ast::ast::JSXAttributeName;
use oxc_span::{SPAN, Span};
use vize_armature::DirectiveNode;

use crate::{parser::ParserImpl, utils::DirectiveExt};

impl<'a> ParserImpl<'a> {
  /// Parse directive name into a [`JSXAttributeName`].
  ///
  /// ### Semantic
  ///  - Treat directive type as namespace, e.g. `v-bind` for `:class="..."`,
  ///    also for `v-for`, `v-if` which have no params.
  ///  - Treat directive argument and modifiers as attribute name, e.g.
  ///    `v-bind:class.a.b` -> `class.a.b`.
  pub(crate) fn parse_directive_name(&self, dire: &DirectiveNode<'_>) -> JSXAttributeName<'a> {
    let span = dire.head_span(self.source_text);

    match span.source_text(self.source_text) {
      name if name.starts_with("v-") => self.analyze_directive_name(name, span),
      name if name.starts_with(':') => self.analyze_directive_alias(name, span, "v-bind"),
      name if name.starts_with('@') => self.analyze_directive_alias(name, span, "v-on"),
      name if name.starts_with('#') => self.analyze_directive_alias(name, span, "v-slot"),
      // SAFETY: vize only emits a Directive prop when the source begins with one of the prefixes above.
      _ => unreachable!(),
    }
  }

  /// For the v-bind:class="..." also for v-model="..." (no params)
  fn analyze_directive_name(&self, name: &'a str, span: Span) -> JSXAttributeName<'a> {
    let name_space_span = Span::new(
      span.start,
      memchr(b':', name.as_bytes()).map_or(span.end, |i| span.start + i as u32),
    );

    let name_span = if name_space_span == span {
      // No ':' in the name, so it's not a namespaced directive — v-for, v-if,
      // v-show, v-html, v-model, v-pre, etc.
      SPAN
    } else {
      Span::new(name_space_span.end + 1, span.end)
    };

    self.ast.jsx_attribute_name_namespaced_name(
      span,
      self.ast.jsx_identifier(name_space_span, name_space_span.source_text(self.source_text)),
      self.ast.jsx_identifier(name_span, name_span.source_text(self.source_text)),
    )
  }

  fn analyze_directive_alias(
    &self,
    name: &'a str,
    span: Span,
    full_name: &'a str,
  ) -> JSXAttributeName<'a> {
    self.ast.jsx_attribute_name_namespaced_name(
      span,
      self.ast.jsx_identifier(Span::sized(span.start, 1), full_name),
      self.ast.jsx_identifier(Span::new(span.start + 1, span.end), &name[1..]),
    )
  }
}

#[cfg(test)]
mod tests {
  use crate::test_ast;

  #[test]
  fn test_parse_directive_name() {
    test_ast!("directive/basic.vue");
  }
}
