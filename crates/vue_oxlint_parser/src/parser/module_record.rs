use oxc_span::SPAN;
use oxc_syntax::module_record::ModuleRecord;
use oxc_syntax::module_record::{ExportEntry, ExportExportName, ExportImportName, ExportLocalName};

use crate::VueParser;

pub fn merge_module_record<'a>(target: &mut ModuleRecord<'a>, mut source: ModuleRecord<'a>) {
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

impl VueParser<'_, '_> {
  pub fn fix_module_records(&mut self) {
    self.module_record.has_module_syntax = true;

    if !self.module_record.local_export_entries.iter().any(|entry| entry.export_name.is_default()) {
      // For no script or <script setup> only file
      self.module_record.local_export_entries.push(ExportEntry {
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
