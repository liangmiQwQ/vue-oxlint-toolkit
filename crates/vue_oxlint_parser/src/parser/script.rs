//! `<script>` / `<script setup>` handling.
//!
//! Phase 3 ports the parser-side utilities from `vue_oxlint_jsx` that phase 4
//! will need when the recursive-descent parser starts crossing script and
//! directive boundaries:
//!
//! - source-type resolution from `lang=...`
//! - duplicate `<script>` / `<script setup>` guards
//! - in-place wrapped `oxc_parser` calls that preserve original-source spans
//! - module-record aggregation
//! - script-comment and token collection

use oxc_allocator::{Allocator, CloneIn, Dummy, TakeIn, Vec as ArenaVec};
use oxc_ast::{
  Comment,
  ast::{Directive, Expression, Program, Statement},
};
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::{Parser, ParserReturn, config::RuntimeParserConfig};
use oxc_span::{GetSpan, SPAN, SourceType, Span};
use oxc_syntax::module_record::{
  ExportEntry, ExportExportName, ExportImportName, ExportLocalName, ModuleRecord,
};

use super::VueParser;

#[allow(dead_code, reason = "phase 4 will call this when it starts parsing <script> tags")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScriptKind {
  Script,
  Setup,
}

impl ScriptKind {
  const fn is_setup(self) -> bool {
    matches!(self, Self::Setup)
  }
}

#[allow(
  dead_code,
  reason = "phase 4 will use these module-record helpers while aggregating script blocks"
)]
pub(super) trait ModuleRecordExt {
  fn merge_all(&mut self, instance: Self);
  fn merge_imports(&mut self, instance: Self);
  fn ensure_default_export(&mut self);
}

impl ModuleRecordExt for ModuleRecord<'_> {
  fn merge_all(&mut self, mut instance: Self) {
    self.has_module_syntax |= instance.has_module_syntax;
    self.requested_modules.extend(instance.requested_modules);
    self.import_entries.append(&mut instance.import_entries);
    self.local_export_entries.append(&mut instance.local_export_entries);
    self.indirect_export_entries.append(&mut instance.indirect_export_entries);
    self.star_export_entries.append(&mut instance.star_export_entries);
    self.exported_bindings.extend(instance.exported_bindings);
    self.dynamic_imports.append(&mut instance.dynamic_imports);
    self.import_metas.append(&mut instance.import_metas);
  }

  fn merge_imports(&mut self, mut instance: Self) {
    self.has_module_syntax |= instance.has_module_syntax;
    self.requested_modules.extend(instance.requested_modules);
    self.import_entries.append(&mut instance.import_entries);
    self.dynamic_imports.append(&mut instance.dynamic_imports);
    self.import_metas.append(&mut instance.import_metas);
  }

  fn ensure_default_export(&mut self) {
    self.has_module_syntax = true;

    if !self.local_export_entries.iter().any(|entry| entry.export_name.is_default()) {
      self.local_export_entries.push(ExportEntry {
        span: SPAN,
        statement_span: SPAN,
        module_request: None,
        import_name: ExportImportName::Null,
        export_name: ExportExportName::Default(SPAN),
        local_name: ExportLocalName::Null,
        is_type: false,
      });
    }
  }
}

#[allow(
  dead_code,
  reason = "phase 4 will call these wrapped oxc_parser helpers from the recursive-descent parser"
)]
impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub(super) fn parse_script_block(
    &mut self,
    span: Span,
    lang: Option<&'a str>,
    kind: ScriptKind,
  ) -> Option<Program<'b>> {
    self.resolve_script_lang(lang)?;

    if span.source_text(self.source_text).trim().is_empty() {
      return Some(Program::dummy(self.js_allocator));
    }

    self.register_script_block(kind, span)?;

    let mut ret = self.parse_program_region(span, &[], &[], self.js_allocator)?;
    self.collect_script_comments(&ret.program.comments);
    self.script_tokens.append(&mut ret.tokens);
    self.record_clean_spans(&ret.program.directives, &ret.program.body);

    if kind.is_setup() {
      self.module_record.merge_imports(ret.module_record);
    } else {
      self.module_record.merge_all(ret.module_record);
    }

    Some(ret.program)
  }

  pub(super) fn parse_pure_expression(
    &mut self,
    span: Span,
    allocator: &'b Allocator,
  ) -> Option<Expression<'b>> {
    self.parse_expression_region(span, b"(", b")", allocator)
  }

  pub(super) fn parse_expression_region(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: &'b Allocator,
  ) -> Option<Expression<'b>> {
    let mut ret = self.parse_program_region(span, start_wrap, end_wrap, allocator)?;
    self.collect_script_comments(&ret.program.comments);

    let stmt = ret.program.body.get_mut(0)?;
    let Statement::ExpressionStatement(stmt) = stmt else {
      unreachable!("wrapped expression regions always parse as an expression statement");
    };
    let Expression::ParenthesizedExpression(expr) = &mut stmt.expression else {
      unreachable!("wrapped expression regions always retain their outer parentheses");
    };
    Some(expr.expression.take_in(allocator))
  }

  pub(super) fn resolve_script_lang(&mut self, lang: Option<&'a str>) -> Option<SourceType> {
    let lang = lang.unwrap_or("js");

    if let Some(prev) = self.script_lang {
      if prev != lang {
        self.errors.push(OxcDiagnostic::error(
          "<script> and <script setup> must have the same language type.",
        ));
        return None;
      }
    } else {
      self.script_lang = Some(lang);
    }

    let Ok(source_type) = SourceType::from_extension(lang) else {
      self
        .errors
        .push(OxcDiagnostic::error(format!("Unsupported lang {lang} in <script> blocks.")));
      return None;
    };

    self.source_type = source_type;
    Some(source_type)
  }

  pub(super) fn register_script_block(&mut self, kind: ScriptKind, span: Span) -> Option<()> {
    let already_set = match kind {
      ScriptKind::Script => &mut self.script_set,
      ScriptKind::Setup => &mut self.script_setup_set,
    };

    if *already_set {
      let message = match kind {
        ScriptKind::Script => "Single file component can contain only one <script> element.",
        ScriptKind::Setup => "Single file component can contain only one <script setup> element.",
      };
      self.errors.push(OxcDiagnostic::error(message).with_label(span));
      return None;
    }

    *already_set = true;
    Some(())
  }

  pub(super) fn parse_program_region(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: &'b Allocator,
  ) -> Option<ParserReturn<'b>> {
    let start = span.start as usize;
    let end = span.end as usize;
    let source_len = self.oxc_source_text.len();

    if start < start_wrap.len() || end + end_wrap.len() > source_len {
      self.errors.push(
        OxcDiagnostic::error("wrapped parser region does not fit inside the SFC source")
          .with_label(span),
      );
      return None;
    }

    // SAFETY: the parser only mutates bytes outside `span`, and resets the
    // scratch buffer before returning.
    unsafe {
      let real_start = start - start_wrap.len();
      let first_byte_ptr = self.mut_ptr_oxc_source_text.cast::<u8>();

      std::ptr::copy_nonoverlapping(
        start_wrap.as_ptr(),
        first_byte_ptr.add(real_start),
        start_wrap.len(),
      );
      std::ptr::copy_nonoverlapping(end_wrap.as_ptr(), first_byte_ptr.add(end), end_wrap.len());

      for i in 0..real_start {
        first_byte_ptr.add(i).write(b' ');
      }
    }

    // SAFETY: the scratch buffer was copied from a valid UTF-8 source and the
    // wrapper bytes are ASCII.
    let result = self.call_oxc_parse(
      unsafe { str::from_utf8_unchecked(&self.oxc_source_text.as_bytes()[..end + end_wrap.len()]) },
      allocator,
    );

    self.sync_source_text();
    result
  }

  fn call_oxc_parse(
    &mut self,
    source: &'b str,
    allocator: &'b Allocator,
  ) -> Option<ParserReturn<'b>> {
    let mut ret = Parser::new(allocator, source, self.source_type)
      .with_options(self.options)
      .with_config(RuntimeParserConfig::new(true))
      .parse();

    self.errors.append(&mut ret.errors);
    if ret.panicked { None } else { Some(ret) }
  }

  fn collect_script_comments(&mut self, comments: &ArenaVec<'b, Comment>) {
    let mut comments = comments.clone_in(self.vue_allocator);
    self.script_comments.append(&mut comments);
  }

  fn record_clean_spans(
    &mut self,
    directives: &ArenaVec<'b, Directive<'b>>,
    statements: &ArenaVec<'b, Statement<'b>>,
  ) {
    for directive in directives {
      self.clean_spans.insert(directive.span());
    }
    for statement in statements {
      self.clean_spans.insert(statement.span());
    }
  }
}
