//! AST-aware codegen with internal hooks.
//!
//! Walks `oxc_ast` directly and prints to an `oxc_data_structures::CodeBuffer`,
//! firing a hook on enter / leave for every node so callers can build per-node
//! source mappings without paying for an estree-JSON round trip.
//!
//! Coverage is intentionally pragmatic: the toolkit's parser produces a
//! Vue-shaped JSX AST, so the printer focuses on the node kinds that show up
//! there. Anything else falls back to pasting the original source slice and
//! recording a single mapping for the parent — output stays readable while we
//! grow coverage incrementally.
#![allow(clippy::too_many_lines)]

#[allow(clippy::wildcard_imports)]
use oxc_ast::ast::*;
use oxc_codegen::Codegen as OxcCodegen;
use oxc_data_structures::code_buffer::CodeBuffer;
use oxc_span::{GetSpan, Span};

/// Receives a callback for every visited AST node. Spans of `0..0` (synthesised
/// nodes from the Vue → JSX transform) are skipped before the hook is invoked.
pub trait CodegenHook {
  fn record(&mut self, span: Span, virtual_start: u32, virtual_end: u32);
}

pub struct Codegen<'a, H: CodegenHook> {
  buf: CodeBuffer,
  hook: H,
  source: &'a str,
}

impl<'a, H: CodegenHook> Codegen<'a, H> {
  pub fn new(source: &'a str, hook: H) -> Self {
    Self { buf: CodeBuffer::new(), hook, source }
  }

  pub fn build(mut self, program: &Program<'a>) -> (String, H) {
    self.print_program(program);
    (self.buf.into_string(), self.hook)
  }

  // -- low level helpers ----------------------------------------------------

  fn pos(&self) -> u32 {
    self.buf.len() as u32
  }

  fn record(&mut self, span: Span, start: u32) {
    if span.is_empty() {
      return;
    }
    let end = self.pos();
    self.hook.record(span, start, end);
  }

  fn write(&mut self, s: &str) {
    self.buf.print_str(s);
  }

  fn write_byte(&mut self, b: u8) {
    self.buf.print_ascii_byte(b);
  }

  fn write_source(&mut self, span: Span) {
    if let Some(slice) = self.source.get(span.start as usize..span.end as usize) {
      self.write(slice);
    }
  }

  /// Emit a node by pasting its source slice. Used as a fallback for node
  /// kinds we don't have a specialized printer for. Only valid when the
  /// node's span points at original JS source (i.e. not synthesised).
  fn paste(&mut self, span: Span) {
    let start = self.pos();
    self.write_source(span);
    self.record(span, start);
  }

  // -- program / statements -------------------------------------------------

  fn print_program(&mut self, program: &Program<'a>) {
    let start = self.pos();
    for (i, stmt) in program.body.iter().enumerate() {
      if i > 0 {
        self.write_byte(b'\n');
      }
      self.print_statement(stmt);
    }
    self.record(program.span, start);
  }

  fn print_statement(&mut self, stmt: &Statement<'a>) {
    match stmt {
      Statement::ExpressionStatement(s) => {
        let start = self.pos();
        self.print_expression(&s.expression);
        self.write_byte(b';');
        self.record(s.span, start);
      }
      Statement::BlockStatement(s) => self.print_block(s),
      Statement::ReturnStatement(s) => {
        let start = self.pos();
        self.write("return");
        if let Some(arg) = &s.argument {
          self.write_byte(b' ');
          self.print_expression(arg);
        }
        self.write_byte(b';');
        self.record(s.span, start);
      }
      Statement::EmptyStatement(s) => {
        let start = self.pos();
        self.write_byte(b';');
        self.record(s.span, start);
      }
      Statement::VariableDeclaration(d) => {
        self.print_variable_decl(d);
        self.write_byte(b';');
      }
      Statement::ImportDeclaration(d) => self.print_import_decl(d),
      Statement::ExportNamedDeclaration(d) => self.print_export_named(d),
      Statement::ExportDefaultDeclaration(d) => self.print_export_default(d),
      Statement::ExportAllDeclaration(d) => self.print_export_all(d),
      // Other statements (control flow, function/class decls, TS decls, etc.)
      // either won't appear at top level of the transformed program or have a
      // span that points at unmodified source — paste it.
      other => self.paste(other.span()),
    }
  }

  fn print_block(&mut self, block: &BlockStatement<'a>) {
    let start = self.pos();
    self.write_byte(b'{');
    for stmt in &block.body {
      self.write_byte(b'\n');
      self.print_statement(stmt);
    }
    if !block.body.is_empty() {
      self.write_byte(b'\n');
    }
    self.write_byte(b'}');
    self.record(block.span, start);
  }

  // -- declarations ---------------------------------------------------------

  fn print_variable_decl(&mut self, decl: &VariableDeclaration<'a>) {
    let start = self.pos();
    self.write(decl.kind.as_str());
    self.write_byte(b' ');
    for (i, d) in decl.declarations.iter().enumerate() {
      if i > 0 {
        self.write(", ");
      }
      self.print_variable_declarator(d);
    }
    self.record(decl.span, start);
  }

  fn print_variable_declarator(&mut self, d: &VariableDeclarator<'a>) {
    let start = self.pos();
    self.print_binding_pattern(&d.id);
    if let Some(ann) = &d.type_annotation {
      self.print_type_annotation(ann);
    }
    if let Some(init) = &d.init {
      self.write(" = ");
      self.print_expression(init);
    }
    self.record(d.span, start);
  }

  fn print_import_decl(&mut self, d: &ImportDeclaration<'a>) {
    let start = self.pos();
    self.write("import ");
    let import_is_type = d.import_kind == ImportOrExportKind::Type;
    if import_is_type {
      self.write("type ");
    }
    if let Some(specifiers) = &d.specifiers {
      let mut default: Option<&ImportDefaultSpecifier> = None;
      let mut namespace: Option<&ImportNamespaceSpecifier> = None;
      let mut named: Vec<&ImportSpecifier> = Vec::new();
      for s in specifiers {
        match s {
          ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => default = Some(s),
          ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => namespace = Some(s),
          ImportDeclarationSpecifier::ImportSpecifier(s) => named.push(s),
        }
      }
      let mut needs_comma = default.is_some_and(|s| {
        let st = self.pos();
        self.print_binding_identifier(&s.local);
        self.record(s.span, st);
        true
      });
      if let Some(s) = namespace {
        if needs_comma {
          self.write(", ");
        }
        let st = self.pos();
        self.write("* as ");
        self.print_binding_identifier(&s.local);
        self.record(s.span, st);
        needs_comma = true;
      }
      if !named.is_empty() {
        if needs_comma {
          self.write(", ");
        }
        self.write("{ ");
        for (i, s) in named.iter().enumerate() {
          if i > 0 {
            self.write(", ");
          }
          let st = self.pos();
          if !import_is_type && s.import_kind == ImportOrExportKind::Type {
            self.write("type ");
          }
          self.print_module_export_name(&s.imported);
          if module_export_name_text(&s.imported) != s.local.name.as_str() {
            self.write(" as ");
            self.print_binding_identifier(&s.local);
          }
          self.record(s.span, st);
        }
        self.write(" }");
      }
      if default.is_some() || namespace.is_some() || !named.is_empty() {
        self.write(" from ");
      }
    }
    self.print_string_literal(&d.source);
    self.print_with_clause(d.with_clause.as_deref());
    self.write_byte(b';');
    self.record(d.span, start);
  }

  fn print_with_clause(&mut self, clause: Option<&WithClause<'a>>) {
    if let Some(clause) = clause {
      self.write_byte(b' ');
      match clause.keyword {
        WithClauseKeyword::With => self.write("with"),
        WithClauseKeyword::Assert => self.write("assert"),
      }
      if !clause.span.is_empty() {
        self.write_byte(b' ');
        self.paste(clause.span);
        return;
      }
      self.write(" { ");
      for (i, attr) in clause.with_entries.iter().enumerate() {
        if i > 0 {
          self.write(", ");
        }
        self.print_import_attribute(attr);
      }
      self.write(" }");
    }
  }

  fn print_import_attribute(&mut self, attr: &ImportAttribute<'a>) {
    let start = self.pos();
    match &attr.key {
      ImportAttributeKey::Identifier(id) => self.print_identifier_name(id),
      ImportAttributeKey::StringLiteral(s) => self.print_string_literal(s),
    }
    self.write(": ");
    self.print_string_literal(&attr.value);
    self.record(attr.span, start);
  }

  fn print_module_export_name(&mut self, name: &ModuleExportName<'a>) {
    match name {
      ModuleExportName::IdentifierName(n) => self.print_identifier_name(n),
      ModuleExportName::IdentifierReference(r) => self.print_identifier_reference(r),
      ModuleExportName::StringLiteral(s) => self.print_string_literal(s),
    }
  }

  fn print_export_named(&mut self, d: &ExportNamedDeclaration<'a>) {
    let start = self.pos();
    self.write("export ");
    if let Some(decl) = &d.declaration {
      match decl {
        Declaration::VariableDeclaration(v) => {
          self.print_variable_decl(v);
          self.write_byte(b';');
        }
        other => self.paste(other.span()),
      }
    } else {
      self.write("{ ");
      for (i, s) in d.specifiers.iter().enumerate() {
        if i > 0 {
          self.write(", ");
        }
        let st = self.pos();
        self.print_module_export_name(&s.local);
        if module_export_name_text(&s.local) != module_export_name_text(&s.exported) {
          self.write(" as ");
          self.print_module_export_name(&s.exported);
        }
        self.record(s.span, st);
      }
      self.write(" }");
      if let Some(src) = &d.source {
        self.write(" from ");
        self.print_string_literal(src);
      }
      self.print_with_clause(d.with_clause.as_deref());
      self.write_byte(b';');
    }
    self.record(d.span, start);
  }

  fn print_export_default(&mut self, d: &ExportDefaultDeclaration<'a>) {
    let start = self.pos();
    self.write("export default ");
    let mut emit_semi = true;
    if let Some(expr) = d.declaration.as_expression() {
      self.print_expression(expr);
    } else {
      // FunctionDeclaration / ClassDeclaration / TS decls — paste source.
      let span = d.declaration.span();
      self.paste(span);
      emit_semi = false;
    }
    if emit_semi {
      self.write_byte(b';');
    }
    self.record(d.span, start);
  }

  fn print_export_all(&mut self, d: &ExportAllDeclaration<'a>) {
    let start = self.pos();
    self.write("export *");
    if let Some(name) = &d.exported {
      self.write(" as ");
      self.print_module_export_name(name);
    }
    self.write(" from ");
    self.print_string_literal(&d.source);
    self.print_with_clause(d.with_clause.as_deref());
    self.write_byte(b';');
    self.record(d.span, start);
  }

  // -- patterns -------------------------------------------------------------

  fn print_binding_pattern(&mut self, p: &BindingPattern<'a>) {
    match p {
      BindingPattern::BindingIdentifier(id) => self.print_binding_identifier(id),
      _ => self.paste(p.span()),
    }
  }

  fn print_binding_identifier(&mut self, id: &BindingIdentifier<'a>) {
    let start = self.pos();
    self.write(id.name.as_str());
    self.record(id.span, start);
  }

  fn print_identifier_name(&mut self, id: &IdentifierName<'a>) {
    let start = self.pos();
    self.write(id.name.as_str());
    self.record(id.span, start);
  }

  fn print_identifier_reference(&mut self, id: &IdentifierReference<'a>) {
    let start = self.pos();
    self.write(id.name.as_str());
    self.record(id.span, start);
  }

  fn print_formal_params(&mut self, params: &FormalParameters<'a>) {
    for (i, p) in params.items.iter().enumerate() {
      if i > 0 {
        self.write(", ");
      }
      self.print_binding_pattern(&p.pattern);
    }
    if let Some(rest) = &params.rest {
      if !params.items.is_empty() {
        self.write(", ");
      }
      let start = self.pos();
      self.write("...");
      self.print_binding_pattern(&rest.rest.argument);
      self.record(rest.span, start);
    }
  }

  // -- expressions ----------------------------------------------------------

  fn print_expression(&mut self, expr: &Expression<'a>) {
    match expr {
      Expression::Identifier(id) => self.print_identifier_reference(id),
      Expression::StringLiteral(s) => self.print_string_literal(s),
      Expression::NumericLiteral(n) => self.paste(n.span),
      Expression::BooleanLiteral(b) => {
        let start = self.pos();
        self.write(if b.value { "true" } else { "false" });
        self.record(b.span, start);
      }
      Expression::NullLiteral(n) => {
        let start = self.pos();
        self.write("null");
        self.record(n.span, start);
      }
      Expression::BigIntLiteral(b) => self.paste(b.span),
      Expression::RegExpLiteral(r) => self.paste(r.span),
      Expression::TemplateLiteral(t) => self.paste(t.span),
      Expression::ArrowFunctionExpression(a) => self.print_arrow(a),
      Expression::JSXElement(e) => self.print_jsx_element(e),
      Expression::JSXFragment(f) => self.print_jsx_fragment(f),
      Expression::ParenthesizedExpression(p) => {
        let start = self.pos();
        self.write_byte(b'(');
        self.print_expression(&p.expression);
        self.write_byte(b')');
        self.record(p.span, start);
      }
      // Fallback: paste the original source. Correct for unmodified script
      // bodies (their span points at original JS source verbatim).
      other => self.paste(other.span()),
    }
  }

  fn print_string_literal(&mut self, s: &StringLiteral<'a>) {
    let start = self.pos();
    if s.raw.is_some() && !s.span.is_empty() {
      self.write_source(s.span);
    } else {
      self.write_byte(b'\'');
      self.write(s.value.as_str());
      self.write_byte(b'\'');
    }
    self.record(s.span, start);
  }

  fn print_arrow(&mut self, a: &ArrowFunctionExpression<'a>) {
    let start = self.pos();
    if a.r#async {
      self.write("async ");
    }
    self.write_byte(b'(');
    self.print_formal_params(&a.params);
    self.write(") => ");
    if a.expression {
      if let Some(Statement::ExpressionStatement(s)) = a.body.statements.first() {
        self.print_expression(&s.expression);
      }
    } else {
      let body_start = self.pos();
      self.write_byte(b'{');
      for stmt in &a.body.statements {
        self.write_byte(b'\n');
        self.print_statement(stmt);
      }
      if !a.body.statements.is_empty() {
        self.write_byte(b'\n');
      }
      self.write_byte(b'}');
      self.record(a.body.span, body_start);
    }
    self.record(a.span, start);
  }

  // -- JSX ------------------------------------------------------------------

  fn print_jsx_element(&mut self, e: &JSXElement<'a>) {
    let start = self.pos();
    self.print_jsx_opening(&e.opening_element, e.closing_element.is_none());
    for child in &e.children {
      self.print_jsx_child(child);
    }
    if let Some(closing) = &e.closing_element {
      self.print_jsx_closing(closing);
    }
    self.record(e.span, start);
  }

  fn print_jsx_fragment(&mut self, f: &JSXFragment<'a>) {
    let start = self.pos();
    let s = self.pos();
    self.write("<>");
    self.record(f.opening_fragment.span, s);
    for child in &f.children {
      self.print_jsx_child(child);
    }
    let s = self.pos();
    self.write("</>");
    self.record(f.closing_fragment.span, s);
    self.record(f.span, start);
  }

  fn print_jsx_opening(&mut self, o: &JSXOpeningElement<'a>, self_closing: bool) {
    let start = self.pos();
    self.write_byte(b'<');
    self.print_jsx_element_name(&o.name);
    for attr in &o.attributes {
      self.write_byte(b' ');
      self.print_jsx_attribute_item(attr);
    }
    if self_closing {
      self.write(" />");
    } else {
      self.write_byte(b'>');
    }
    self.record(o.span, start);
  }

  fn print_jsx_closing(&mut self, c: &JSXClosingElement<'a>) {
    let start = self.pos();
    if jsx_element_name_is_empty(&c.name) {
      self.write("</>");
    } else {
      self.write("</");
      self.print_jsx_element_name(&c.name);
      self.write_byte(b'>');
    }
    self.record(c.span, start);
  }

  fn print_jsx_element_name(&mut self, name: &JSXElementName<'a>) {
    match name {
      JSXElementName::Identifier(id) => self.print_jsx_identifier(id),
      JSXElementName::IdentifierReference(r) => self.print_identifier_reference(r),
      JSXElementName::NamespacedName(n) => self.print_jsx_namespaced(n),
      JSXElementName::MemberExpression(m) => self.print_jsx_member(m),
      JSXElementName::ThisExpression(t) => {
        let start = self.pos();
        self.write("this");
        self.record(t.span, start);
      }
    }
  }

  fn print_jsx_identifier(&mut self, id: &JSXIdentifier<'a>) {
    let start = self.pos();
    self.write(id.name.as_str());
    self.record(id.span, start);
  }

  fn print_jsx_namespaced(&mut self, n: &JSXNamespacedName<'a>) {
    let start = self.pos();
    self.print_jsx_identifier(&n.namespace);
    self.write_byte(b':');
    self.print_jsx_identifier(&n.name);
    self.record(n.span, start);
  }

  fn print_jsx_member(&mut self, m: &JSXMemberExpression<'a>) {
    let start = self.pos();
    match &m.object {
      JSXMemberExpressionObject::IdentifierReference(r) => self.print_identifier_reference(r),
      JSXMemberExpressionObject::MemberExpression(inner) => self.print_jsx_member(inner),
      JSXMemberExpressionObject::ThisExpression(t) => {
        let st = self.pos();
        self.write("this");
        self.record(t.span, st);
      }
    }
    self.write_byte(b'.');
    self.print_jsx_identifier(&m.property);
    self.record(m.span, start);
  }

  fn print_jsx_attribute_item(&mut self, item: &JSXAttributeItem<'a>) {
    match item {
      JSXAttributeItem::Attribute(a) => {
        let start = self.pos();
        self.print_jsx_attribute_name(&a.name);
        if let Some(value) = &a.value {
          self.write_byte(b'=');
          self.print_jsx_attribute_value(value);
        }
        self.record(a.span, start);
      }
      JSXAttributeItem::SpreadAttribute(s) => {
        let start = self.pos();
        self.write("{...");
        self.print_expression(&s.argument);
        self.write_byte(b'}');
        self.record(s.span, start);
      }
    }
  }

  fn print_jsx_attribute_name(&mut self, name: &JSXAttributeName<'a>) {
    match name {
      JSXAttributeName::Identifier(id) => self.print_jsx_identifier(id),
      JSXAttributeName::NamespacedName(n) => self.print_jsx_namespaced(n),
    }
  }

  fn print_jsx_attribute_value(&mut self, value: &JSXAttributeValue<'a>) {
    match value {
      JSXAttributeValue::StringLiteral(s) => {
        let start = self.pos();
        if s.raw.is_some() && !s.span.is_empty() {
          self.write_source(s.span);
        } else {
          self.write_byte(b'"');
          self.write(s.value.as_str());
          self.write_byte(b'"');
        }
        self.record(s.span, start);
      }
      JSXAttributeValue::ExpressionContainer(c) => self.print_jsx_expr_container(c),
      JSXAttributeValue::Element(e) => self.print_jsx_element(e),
      JSXAttributeValue::Fragment(f) => self.print_jsx_fragment(f),
    }
  }

  fn print_jsx_expr_container(&mut self, c: &JSXExpressionContainer<'a>) {
    let start = self.pos();
    self.write_byte(b'{');
    self.print_jsx_expression(&c.expression);
    self.write_byte(b'}');
    self.record(c.span, start);
  }

  fn print_jsx_expression(&mut self, expr: &JSXExpression<'a>) {
    if matches!(expr, JSXExpression::EmptyExpression(_)) {
      return;
    }
    let expression = expr.to_expression();
    if expression.span().is_empty() {
      let mut codegen = OxcCodegen::new();
      codegen.print_expression(expression);
      self.write(&codegen.into_source_text());
    } else {
      self.paste(expression.span());
    }
  }

  fn print_jsx_child(&mut self, child: &JSXChild<'a>) {
    match child {
      JSXChild::Text(t) => {
        let start = self.pos();
        self.write(t.value.as_str());
        self.record(t.span, start);
      }
      JSXChild::Element(e) => self.print_jsx_element(e),
      JSXChild::Fragment(f) => self.print_jsx_fragment(f),
      JSXChild::ExpressionContainer(c) => self.print_jsx_expr_container(c),
      JSXChild::Spread(s) => {
        let start = self.pos();
        self.write("{...");
        self.print_expression(&s.expression);
        self.write_byte(b'}');
        self.record(s.span, start);
      }
    }
  }

  // -- TypeScript -----------------------------------------------------------

  fn print_type_annotation(&mut self, ann: &TSTypeAnnotation<'a>) {
    let start = self.pos();
    self.write(": ");
    self.print_ts_type(&ann.type_annotation);
    self.record(ann.span, start);
  }

  fn print_ts_type(&mut self, ty: &TSType<'a>) {
    self.paste(ty.span());
  }
}

// -- helpers ----------------------------------------------------------------

fn jsx_element_name_is_empty(name: &JSXElementName<'_>) -> bool {
  match name {
    JSXElementName::Identifier(id) => id.name.is_empty(),
    _ => false,
  }
}

fn module_export_name_text<'a>(name: &'a ModuleExportName<'_>) -> &'a str {
  match name {
    ModuleExportName::IdentifierName(n) => n.name.as_str(),
    ModuleExportName::IdentifierReference(r) => r.name.as_str(),
    ModuleExportName::StringLiteral(s) => s.value.as_str(),
  }
}
