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
use crate::parser::irregular_whitespaces::collect_irregular_whitespaces;
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
          irregular_whitespaces: collect_irregular_whitespaces(source_text),
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
        irregular_whitespaces: Box::new([]),
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
        // Template text nodes are intentionally ignored.
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
    let _ = text_start;

    // Parse the skip ones
    let mut children: ArenaVec<'a, JSXChild<'a>> = self.ast.vec();

    for child in raw_children {
      children.push(match child {
        ParsingChild::Finish(child) => child,
        ParsingChild::Skip(node) => {
          if node.tag_name == "template" {
            self.parse_element(node, None).0
          } else {
            self.parse_element(node, Some(self.ast.vec())).0
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

  test_ast!(basic_vue, "basic.vue");
  test_ast!(typescript_vue, "typescript.vue");
  test_ast!(void_vue, "void.vue", true, false);
  test_ast!(tags_vue, "tags.vue");
  test_ast!(root_texts_vue, "root_texts.vue");
  test_ast!(components_vue, "components.vue");
  test_ast!(comments_vue, "comments.vue");
  test_ast!(error_template_vue, "error/template.vue", true, true);
  test_ast!(error_interpolation_vue, "error/interpolation.vue", true, true);
  test_ast!(error_script_vue, "error/script.vue", true, false);
  test_ast!(error_directive_vue, "error/directive.vue", true, false);
  test_ast!(error_multiple_langs_vue, "error/multiple_langs.vue", true, true);
  test_ast!(error_multiple_scripts_vue, "error/multiple_scripts.vue", true, true);
  test_ast!(error_empty_multiple_scripts_vue, "error/empty_multiple_scripts.vue");
  test_ast!(scripts_basic_vue, "scripts/basic.vue");
  test_ast!(scripts_setup_vue, "scripts/setup.vue");
  test_ast!(scripts_both_vue, "scripts/both.vue");
  test_ast!(scripts_empty_vue, "scripts/empty.vue");
  test_ast!(scripts_directives_vue, "scripts/directives.vue");
}
