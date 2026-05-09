use memchr::memchr;
use oxc_ast::ast::JSXAttributeName;
use oxc_span::{SPAN, Span};
use vue_compiler_core::parser::Directive;

use crate::parser::{ParserImpl, parse::SourceLocatonSpan};

impl<'a> ParserImpl<'a> {
  /// Parse directive name
  ///
  /// ### Semantic
  ///  - Treat directive type as namespace, like `v-bind` for `:class="..."`, also for `v-for`, `v-if` which has no params
  ///  - Treat directive argument, modifiers as attribute name, like `v-bind:class.a.b` -> `class.a.b`
  pub(crate) fn parse_directive_name(&self, dire: &Directive<'a>) -> JSXAttributeName<'a> {
    let span = dire.head_loc.span();

    match span.source_text(self.source_text) {
      name if name.starts_with("v-") => self.analyze_directive_name(name, span),
      name if name.starts_with(':') => self.analyze_directive_alias(name, span, "v-bind"),
      name if name.starts_with('@') => self.analyze_directive_alias(name, span, "v-on"),
      name if name.starts_with('#') => self.analyze_directive_alias(name, span, "v-slot"),
      // SAFETY: if the directive doesn't start with 'v-', ':', '@', '#', it will be not regarded as a directive by vue-compiler-core
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
      // Can't find ':' in the name, so it's not a namespaced name
      // Such as v-for, v-if, v-else, v-else-if, v-show, v-cloak, v-once, v-pre, v-text, v-html, v-bind, v-on, v-model, v-slot, v-memo, v-transition, v-transition-group, v-custom-directive
      SPAN
    } else {
      Span::new(name_space_span.end + 1, span.end)
    };

    self.ast.jsx_attribute_name_namespaced_name(
      span,
      self.ast.jsx_identifier(
        name_space_span,
        self.codegen_directive_identifier(name_space_span.source_text(self.source_text)),
      ),
      self.ast.jsx_identifier(
        name_span,
        self.codegen_directive_identifier(name_span.source_text(self.source_text)),
      ),
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
      self.ast.jsx_identifier(
        Span::new(span.start + 1, span.end),
        self.codegen_directive_identifier(&name[1..]),
      ),
    )
  }
}
