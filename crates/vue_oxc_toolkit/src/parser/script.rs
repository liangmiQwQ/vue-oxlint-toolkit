use std::collections::HashSet;

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::Statement;

use oxc_span::SourceType;
use vize_armature::{ElementNode, PropNode};

use crate::parser::{ParserImpl, ResParse, ResParseExt, error, modules::Merge};
use crate::utils::{VizeSpan, element_close_span};

impl<'a> ParserImpl<'a> {
  pub fn parse_script(
    &mut self,
    node: &ElementNode<'_>,
    source_types: &mut HashSet<&'a str>,
  ) -> ResParse<()> {
    let lang = node
      .props
      .iter()
      .find_map(|p| match p {
        PropNode::Attribute(attr) if attr.name.as_str() == "lang" => {
          attr.value.as_ref().map(|v| v.loc.span().source_text(self.source_text))
        }
        _ => None,
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

    // Use inner_loc if available, otherwise fall back to children
    let inner_span = node.inner_loc.as_ref().map_or_else(
      || {
        node.children.first().map(|child| {
          let span_start = child.loc().start.offset;
          let span_end = node.children.last().unwrap().loc().end.offset;
          oxc_span::Span::new(span_start, span_end)
        })
      },
      |inner| Some(inner.span()),
    );

    if let Some(span) = inner_span {
      let source = span.source_text(self.source_text);

      if source.trim().is_empty() {
        return ResParse::success(());
      }

      let is_setup = node.props.iter().any(|p| match p {
        PropNode::Attribute(attr) => attr.name.as_str() == "setup",
        PropNode::Directive(dir) => dir.name.as_str() == "setup",
      });

      // Use full element span (opening + body + closing) for a better diagnostic location
      let node_span = {
        let close = element_close_span(self.source_text, node.loc.end.offset, "script");
        if close.is_empty() {
          node.loc.span()
        } else {
          oxc_span::Span::new(node.loc.start.offset, close.end)
        }
      };
      if is_setup {
        if self.setup_set {
          error::multiple_script_setup_tags(&mut self.errors, node_span);
          return ResParse::panic();
        }
        self.setup_set = true;
      } else {
        if self.script_set {
          error::multiple_script_tags(&mut self.errors, node_span);
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
