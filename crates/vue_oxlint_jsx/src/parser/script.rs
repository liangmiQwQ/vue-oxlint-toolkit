use std::collections::HashSet;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::Statement;

use oxc_span::SourceType;
use vue_compiler_core::{
  parser::{ElemProp, Element},
  util::{find_prop, prop_finder},
};

use crate::parser::{
  ParserImpl, ResParse, ResParseExt, error, modules::Merge, parse::SourceLocatonSpan,
};

impl<'a> ParserImpl<'a> {
  pub fn parse_script(
    &mut self,
    node: &Element<'a>,
    source_types: &mut HashSet<&'a str>,
  ) -> ResParse<()> {
    let lang = find_prop(node, "lang")
      .and_then(|p| match p.get_ref() {
        ElemProp::Attr(p) => p.value.as_ref().map(|value| value.content.raw),
        ElemProp::Dir(_) => None,
      })
      .unwrap_or("js");

    source_types.insert(lang);

    if source_types.len() > 1 {
      error::multiple_script_langs(&mut self.errors);
      return ResParse::panic();
    }

    if let Ok(source_type) = SourceType::from_extension(lang) {
      self.source_type = source_type;
    } else {
      error::unexpected_script_lang(&mut self.errors, lang);
      return ResParse::panic();
    }

    // If there is at least one statements in the box
    if let Some(child) = node.children.first() {
      let span = child.get_location().span();
      let source = span.source_text(self.source_text);

      if source.trim().is_empty() {
        return ResParse::success(());
      }

      let is_setup = prop_finder(node, "setup").allow_empty().find().is_some();
      // Handle error if there are multiple script tags
      if is_setup {
        if self.setup_set {
          error::multiple_script_setup_tags(&mut self.errors, node.location.span());
          return ResParse::panic();
        }
        self.setup_set = true;
      } else {
        if self.script_set {
          error::multiple_script_tags(&mut self.errors, node.location.span());
          return ResParse::panic();
        }
        self.script_set = true;
      }

      let Some((mut directives, mut body, module_record)) = self.oxc_parse(span, &[], &[], None)
      else {
        return ResParse::success(());
      };

      // Deal with modules record there
      if is_setup {
        // Only merge imports, as exports are not allowed in <script setup>
        self.module_record.merge_imports(module_record);

        // Append directives to setup block
        self.setup.directives.append(&mut directives);

        // Split imports and other statements
        let mut imports: ArenaVec<Statement<'a>> = self.ast.vec();
        let mut statements: ArenaVec<Statement<'a>> = self.ast.vec();

        for statement in body {
          match statement {
            Statement::ImportDeclaration(_) => imports.push(statement),
            _ => statements.push(statement),
          }
        }

        // Append imports to global statements (top level)
        imports.append(&mut self.global.statements);
        self.global.statements = imports;
        // Replace setup statements with the rest (inside function).
        self.setup.statements = statements;
      } else {
        self.global.directives.append(&mut directives);
        self.module_record.merge(module_record);
        // Append all statements, do not replace all as probably exist imports statements
        self.global.statements.append(&mut body);
      }
    }

    ResParse::success(())
  }
}
