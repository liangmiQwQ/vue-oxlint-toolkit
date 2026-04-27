use oxc_span::SPAN;
use oxc_syntax::module_record::{
  ExportEntry, ExportExportName, ExportImportName, ExportLocalName, ModuleRecord,
};

use crate::parser::ParserImpl;

pub trait Merge: Sized {
  fn merge_imports(&mut self, instance: Self);
  fn merge(&mut self, instance: Self);
}

impl Merge for ModuleRecord<'_> {
  fn merge(&mut self, mut instance: Self) {
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
}

impl ParserImpl<'_> {
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
  use crate::test_module_record;

  #[test]
  fn basic() {
    test_module_record!("modules/basic.vue");
    test_module_record!("modules/import.vue");
    test_module_record!("modules/no-imports.vue");
  }

  #[test]
  fn setup() {
    test_module_record!("modules/setup.vue");
  }
}
