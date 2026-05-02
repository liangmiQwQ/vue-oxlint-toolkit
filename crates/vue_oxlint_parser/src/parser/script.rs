//! Script element parsing: parse `<script>` and `<script setup>` blocks.

use oxc_diagnostics::OxcDiagnostic;
use oxc_span::{GetSpan, SourceType, Span};

use crate::ast::{VAttrOrDirective, VStartTag};
use crate::parser::Parser;

impl<'a> Parser<'a> {
  /// After parsing the body of a `<script>` element (as raw text), parse the JS inside.
  /// Returns the `Program` for storage on `VElement`.
  pub fn parse_script_content(
    &mut self,
    start_tag: &VStartTag<'a>,
    content_span: Span,
  ) -> Option<oxc_ast::ast::Program<'a>> {
    // Determine if this is `<script setup>`
    let is_setup = has_setup_attr(start_tag);

    // Determine language/source_type from `lang` attribute
    let lang = get_lang_attr(self, start_tag).unwrap_or("js");

    // Update source_type (error on multiple conflicting langs)
    if let Ok(source_type) = SourceType::from_extension(lang) {
      self.source_type = source_type;
    } else {
      self.errors.push(OxcDiagnostic::error(format!("Unsupported script lang: {lang}")));
      return None;
    }

    // Enforce single script / single script setup
    if is_setup {
      if self.setup_set {
        self.errors.push(OxcDiagnostic::error("Multiple <script setup> tags found"));
        return None;
      }
      self.setup_set = true;
    } else {
      if self.script_set {
        self.errors.push(OxcDiagnostic::error("Multiple <script> tags found"));
        return None;
      }
      self.script_set = true;
    }

    let source = &self.source_text[content_span.start as usize..content_span.end as usize];
    if source.trim().is_empty() {
      return None;
    }

    let (program, module_record) = self.oxc_parse_script(content_span)?;

    // Populate clean_spans for all top-level directives and statements
    for directive in &program.directives {
      self.clean_spans.insert(directive.span());
    }
    for stmt in &program.body {
      self.clean_spans.insert(stmt.span());
    }

    // Update module_record
    if is_setup {
      merge_imports(&mut self.module_record, module_record);
    } else {
      merge_full(&mut self.module_record, module_record);
    }

    Some(program)
  }
}

fn has_setup_attr(tag: &VStartTag<'_>) -> bool {
  for attr in &tag.attributes {
    if let VAttrOrDirective::Attribute(a) = attr
      && a.name == "setup"
    {
      return true;
    }
  }
  false
}

fn get_lang_attr<'a>(parser: &Parser<'a>, tag: &VStartTag<'a>) -> Option<&'a str> {
  for attr in &tag.attributes {
    if let VAttrOrDirective::Attribute(a) = attr
      && a.name == "lang"
      && let Some(val) = &a.value
    {
      let span = val.span;
      return Some(parser.slice(span.start, span.end));
    }
  }
  None
}

fn merge_imports<'a>(
  target: &mut oxc_syntax::module_record::ModuleRecord<'a>,
  mut source: oxc_syntax::module_record::ModuleRecord<'a>,
) {
  target.has_module_syntax |= source.has_module_syntax;
  target.requested_modules.extend(source.requested_modules);
  target.import_entries.append(&mut source.import_entries);
  target.dynamic_imports.append(&mut source.dynamic_imports);
  target.import_metas.append(&mut source.import_metas);
}

fn merge_full<'a>(
  target: &mut oxc_syntax::module_record::ModuleRecord<'a>,
  mut source: oxc_syntax::module_record::ModuleRecord<'a>,
) {
  target.has_module_syntax |= source.has_module_syntax;
  target.requested_modules.extend(source.requested_modules);
  target.import_entries.append(&mut source.import_entries);
  target.local_export_entries.append(&mut source.local_export_entries);
  target.indirect_export_entries.append(&mut source.indirect_export_entries);
  target.star_export_entries.append(&mut source.star_export_entries);
  target.exported_bindings.extend(source.exported_bindings);
  target.dynamic_imports.append(&mut source.dynamic_imports);
  target.import_metas.append(&mut source.import_metas);
}
