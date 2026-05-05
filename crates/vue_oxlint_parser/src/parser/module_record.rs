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

#[cfg(test)]
mod tests {
  use std::{fs, path::Path};

  use oxc_allocator::Allocator;
  use oxc_syntax::module_record::ModuleRecord;

  use crate::VueParser;

  #[test]
  fn basic() {
    assert_module_record("modules/basic.vue", true);
    assert_module_record("modules/import.vue", true);
    assert_module_record("modules/no-imports.vue", false);
  }

  #[test]
  fn setup() {
    assert_module_record("modules/setup.vue", true);
  }

  fn assert_module_record(relative_fixture: &str, has_vue_import: bool) {
    with_module_record(relative_fixture, |record| {
      assert!(record.has_module_syntax);
      assert!(record.local_export_entries.iter().any(|entry| entry.export_name.is_default()));
      assert_eq!(has_requested_module(record, "vue"), has_vue_import);
    });
  }

  fn with_module_record(relative_fixture: &str, assert_record: impl FnOnce(&ModuleRecord<'_>)) {
    let source = fs::read_to_string(
      Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(relative_fixture),
    )
    .expect("fixture should be readable");

    let vue_allocator = Allocator::default();
    let js_allocator = Allocator::default();
    let ret = VueParser::new(&vue_allocator, &js_allocator, &source).parse();
    assert!(!ret.panicked, "{relative_fixture} should parse without panicking");
    assert_record(&ret.module_record);
  }

  fn has_requested_module(record: &ModuleRecord<'_>, name: &str) -> bool {
    record.requested_modules.keys().any(|module| module.as_str() == name)
  }
}
