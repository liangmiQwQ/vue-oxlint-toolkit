use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashSet;

use oxc_allocator::{self, Dummy, Vec as ArenaVec};
use oxc_ast::ast::{Directive, Expression, FormalParameterKind, JSXChild, Program, Statement};
use oxc_ast::{AstBuilder, NONE};

use oxc_span::{SPAN, Span};
use oxc_syntax::module_record::ModuleRecord;
use vue_compiler_core::SourceLocation;
use vue_compiler_core::parser::{AstNode, Element, ParseOption, Parser, WhitespaceStrategy};
use vue_compiler_core::scanner::{ScanOption, Scanner, TextMode};

use crate::is_void_tag;
use crate::parser::error::OxcErrorHandler;
use crate::parser::{ResParse, ResParseExt};

use super::ParserImpl;
use super::ParserImplReturn;

macro_rules! get_text_mode {
  ($name: expr) => {
    match $name {
      "textarea" => TextMode::RcData,
      "iframe" | "xmp" | "noembed" | "noframes" | "noscript" | "script" | "style" => {
        TextMode::RawText
      }
      _ => TextMode::Data,
    }
  };
}

impl<'a> ParserImpl<'a> {
  pub fn parse(mut self) -> ParserImplReturn<'a> {
    let result = self.analyze();
    match result {
      Ok(()) => {
        self.fix_module_records();

        let Self {
          source_text,
          ast,
          module_record,
          source_type,
          comments,
          errors,
          global,
          setup,
          sfc_struct_jsx_statement: sfc_return,
          ..
        } = self;

        ParserImplReturn {
          program: ast.program(
            Span::new(0, self.source_text.len() as u32),
            source_type.with_jsx(true),
            source_text,
            comments,
            None, // no hashbang needed for vue files
            global.directives,
            Self::get_body_statements(
              global.statements,
              setup.statements,
              setup.directives,
              sfc_return,
              ast,
            ),
          ),
          fatal: false,
          errors,
          module_record,
        }
      }
      Err(()) => ParserImplReturn {
        program: Program::dummy(self.allocator),
        fatal: true,
        errors: self.errors,
        module_record: ModuleRecord::new(self.allocator),
      },
    }
  }

  fn get_body_statements(
    mut statements: ArenaVec<'a, Statement<'a>>,
    mut setup: ArenaVec<'a, Statement<'a>>,
    setup_directives: ArenaVec<'a, Directive<'a>>,
    sfc_return: Option<Statement<'a>>,
    ast: AstBuilder<'a>,
  ) -> ArenaVec<'a, Statement<'a>> {
    if let Some(ret) = sfc_return {
      setup.push(ret);
    }

    let params = ast.alloc_formal_parameters(
      SPAN,
      FormalParameterKind::ArrowFormalParameters,
      ast.vec(),
      NONE,
    );

    let body = ast.alloc_function_body(SPAN, setup_directives, setup);

    statements.push(ast.statement_expression(
      SPAN,
      Expression::ArrowFunctionExpression(
        ast.alloc_arrow_function_expression(SPAN, false, true, NONE, params, NONE, body),
      ),
    ));

    statements
  }
}

enum ParsingChild<'a> {
  Finish(JSXChild<'a>),
  Skip(Element<'a>),
}

impl<'a> ParserImpl<'a> {
  fn analyze(&mut self) -> ResParse<()> {
    let parser = Parser::new(ParseOption {
      whitespace: WhitespaceStrategy::Preserve,
      is_void_tag: |name| is_void_tag!(name),
      get_text_mode: |name| get_text_mode!(name),
      ..Default::default()
    });
    let scanner =
      Scanner::new(ScanOption { get_text_mode: |name| get_text_mode!(name), ..Default::default() });

    // error processing
    let errors = RefCell::from(&mut self.errors);
    let panicked = RefCell::from(false);
    // get ast from vue-compiler-core
    let tokens = scanner.scan(self.source_text, OxcErrorHandler::new(&errors, &panicked));
    let result = parser.parse(tokens, OxcErrorHandler::new(&errors, &panicked));

    if *panicked.borrow() {
      return ResParse::panic();
    }

    let mut raw_children = vec![];
    let mut text_start: u32 = 0;
    let mut source_types: HashSet<&str> = HashSet::new();
    for child in result.children {
      if let AstNode::Element(node) = child {
        // Process the texts between last element and current element
        self.push_text_child(
          &mut raw_children,
          Span::new(text_start, node.location.start.offset as u32),
        );
        text_start = node.location.end.offset as u32;

        raw_children.push(if node.tag_name == "script" {
          // Fill self.global, self.setup
          self.parse_script(&node, &mut source_types)?;
          ParsingChild::Finish(self.parse_element(node, Some(self.ast.vec())).0)
        } else {
          ParsingChild::Skip(node)
        });
      }
    }
    // Process the texts after last element
    self.push_text_child(&mut raw_children, Span::new(text_start, self.source_text.len() as u32));

    // Parse the skip ones
    let mut children: ArenaVec<'a, JSXChild<'a>> = self.ast.vec();

    for child in raw_children {
      children.push(match child {
        ParsingChild::Finish(child) => child,
        ParsingChild::Skip(node) => {
          if node.tag_name == "template" {
            self.parse_element(node, None).0
          } else {
            // Process other tags like <style>
            let text = if let Some(first) = node.children.first() {
              let last = node.children.last().unwrap(); // SAFETY: if first exists, last must exist
              let span = Span::new(
                first.get_location().start.offset as u32,
                last.get_location().end.offset as u32,
              );

              self.ast.vec1(self.jsx_child_text(span, span.source_text(self.source_text)))
            } else {
              self.ast.vec()
            };

            self.parse_element(node, Some(text)).0
          }
        }
      });
    }

    self.sort_errors_and_commends();

    self.sfc_struct_jsx_statement = Some(self.ast.statement_expression(
      SPAN,
      self.ast.expression_jsx_fragment(
        SPAN,
        self.ast.jsx_opening_fragment(SPAN),
        children,
        self.ast.jsx_closing_fragment(SPAN),
      ),
    ));

    ResParse::success(())
  }

  fn sort_errors_and_commends(&mut self) {
    self.comments.sort_by_key(|a| a.span.start);
    self.errors.sort_by(|a, b| {
      let Some(a_labels) = &a.labels else { return Ordering::Less };
      let Some(b_labels) = &b.labels else { return Ordering::Greater };

      let Some(a_first) = a_labels.first() else { return Ordering::Less };
      let Some(b_first) = b_labels.first() else { return Ordering::Greater };

      a_first.offset().cmp(&b_first.offset())
    });
  }

  fn push_text_child(&self, children: &mut Vec<ParsingChild<'a>>, span: Span) {
    if !span.is_empty() {
      children
        .push(ParsingChild::Finish(self.jsx_child_text(span, span.source_text(self.source_text))));
    }
  }
}

// Easy transform from vue_compiler_core::SourceLocation to oxc_span::Span
pub trait SourceLocatonSpan {
  fn span(&self) -> Span;
}

impl SourceLocatonSpan for SourceLocation {
  fn span(&self) -> Span {
    Span::new(self.start.offset as u32, self.end.offset as u32)
  }
}

#[cfg(test)]
mod tests {
  use crate::test_ast;

  #[test]
  fn basic_vue() {
    test_ast!("basic.vue");
    test_ast!("typescript.vue");
    test_ast!("void.vue", true, false);
    test_ast!("tags.vue");
    test_ast!("root_texts.vue");
    test_ast!("components.vue");
  }

  #[test]
  fn comments() {
    test_ast!("comments.vue");
  }

  #[test]
  fn errors() {
    test_ast!("error/template.vue", true, true);
    test_ast!("error/interpolation.vue", true, true);
    test_ast!("error/script.vue", true, false);
    test_ast!("error/directive.vue", true, false);
    test_ast!("error/script.vue", true, false);
    test_ast!("error/directive.vue", true, false);
    test_ast!("error/multiple_langs.vue", true, true);
    test_ast!("error/multiple_scripts.vue", true, true);
    test_ast!("error/empty_multiple_scripts.vue");
  }

  #[test]
  fn scripts() {
    test_ast!("scripts/basic.vue");
    test_ast!("scripts/setup.vue");
    test_ast!("scripts/both.vue");
    test_ast!("scripts/empty.vue");
    test_ast!("scripts/directives.vue");
  }
}
