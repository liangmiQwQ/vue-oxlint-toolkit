use std::ptr;

use memchr::memmem::{find, rfind};
use oxc_allocator::{Allocator, CloneIn, TakeIn, Vec as ArenaVec};
use oxc_ast::ast::{Directive, Expression, Program, Statement};
use oxc_ast_visit::utf8_to_utf16::Utf8ToUtf16;
use oxc_estree_tokens::{ESTreeTokenOptions, to_estree_tokens_json};
use oxc_parser::config::TokensParserConfig;
use oxc_semantic::SemanticBuilder;
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::ModuleRecord;

use crate::{VueParser, ast::Reference};

pub(super) struct OxcParseReturn<'a, 'b> {
  pub(super) directives: ArenaVec<'b, Directive<'b>>,
  pub(super) statements: ArenaVec<'b, Statement<'b>>,
  pub(super) module_record: ModuleRecord<'b>,
  pub(super) references: ArenaVec<'a, Reference<'a>>,
  pub(super) tokens: &'a str,
}

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub(super) fn parse_pure_expression(
    &mut self,
    span: Span,
  ) -> Option<(Expression<'b>, ArenaVec<'a, Reference<'a>>, &'a str)> {
    let allocator = Allocator::new();
    // SAFETY: the wrappers form a parenthesized expression.
    let (expr, references, tokens) =
      unsafe { self.parse_expression(span, b"(", b")", &allocator) }?;
    Some((expr.clone_in(self.js_allocator), references, tokens))
  }

  /// Parse expression with [`oxc_parser`].
  ///
  /// ## Safety
  /// - `start_wrap` must start with `(`.
  /// - `end_wrap` must end with `)`.
  pub(super) unsafe fn parse_expression<'c>(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: &'c Allocator,
  ) -> Option<(Expression<'c>, ArenaVec<'a, Reference<'a>>, &'a str)>
  where
    'b: 'c,
  {
    let OxcParseReturn { mut statements, references, tokens, .. } =
      self.oxc_parse(span, start_wrap, end_wrap, Some(allocator))?;

    let Some(Statement::ExpressionStatement(stmt)) = statements.get_mut(0) else {
      // SAFETY: every caller wraps the input as an expression statement before parsing.
      unreachable!("wrapped expressions parse as expression statements")
    };
    let Expression::ParenthesizedExpression(expression) = &mut stmt.expression else {
      // SAFETY: `parse_expression` requires wrappers that produce a parenthesized expression.
      unreachable!("wrapped expressions parse as parenthesized expressions")
    };

    let tokens = tokens.as_bytes();
    let start_needle = format!(r#""end":{}}},"#, span.start);
    let end_needle = format!(r#""end":{}}}"#, span.end);
    let tokens = if let Some(start) = find(tokens, start_needle.as_bytes())
      && let Some(end) = rfind(tokens, end_needle.as_bytes())
    {
      let start = start + start_needle.len();
      let end = end + end_needle.len();
      // SAFETY: the token slice comes from a JSON string produced by oxc.
      let tokens = unsafe { str::from_utf8_unchecked(&tokens[start..end]) };
      tokens.strip_prefix(',').unwrap_or(tokens)
    } else {
      ""
    };
    Some((expression.expression.take_in(allocator), references, tokens))
  }

  /// Call [`oxc_parser::Parser::parse`] with a custom wrap
  /// Everything before `start` and `start_wrap` will be ignored
  ///
  /// If you need to parse with any wrapper, it will produce unused AST nodes
  /// `allocator` param in `'c` lifetime should provided and drop unused AST nodes as a temporary Arena
  pub(super) fn oxc_parse<'c>(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: Option<&'c Allocator>,
  ) -> Option<OxcParseReturn<'a, 'c>>
  where
    'b: 'c,
  {
    let start = span.start as usize;
    let end = span.end as usize;

    // SAFETY: we don't edit between `start` and `end`, and reset before returning
    unsafe {
      let real_start = start - start_wrap.len();
      let first_byte_ptr = self.mut_ptr_source_text.cast::<u8>();

      // Copy start_wrap to the front of the source text
      ptr::copy_nonoverlapping(
        start_wrap.as_ptr(),
        first_byte_ptr.add(real_start),
        start_wrap.len(),
      );
      // Copy end_wrap to the end of the source text
      ptr::copy_nonoverlapping(end_wrap.as_ptr(), first_byte_ptr.add(end), end_wrap.len());

      // Pad source with space
      for i in 0..real_start {
        first_byte_ptr.add(i).write(b' ');
      }
    }

    // SAFETY: it must be a valid utf-8 string
    let result = self.call_oxc_parse(
      unsafe { str::from_utf8_unchecked(&self.source_text.as_bytes()[..end + end_wrap.len()]) },
      allocator.unwrap_or(self.js_allocator),
    );

    // Reset
    self.sync_source_text();
    result
  }

  fn call_oxc_parse<'c>(
    &mut self,
    source: &'c str,
    allocator: &'c Allocator,
  ) -> Option<OxcParseReturn<'a, 'c>> {
    let source_type =
      self.sfc.source_type.unwrap_or_else(|| SourceType::mjs().with_unambiguous(true));
    let mut ret = oxc_parser::Parser::new(allocator, source, source_type)
      .with_options(self.options)
      .with_config(TokensParserConfig)
      .parse();

    self.errors.append(&mut ret.errors);
    if ret.panicked {
      None
    } else {
      let references = self.collect_semantic_references(&ret.program);
      let mut comments = ret.program.comments.clone_in(self.js_allocator);
      self.sfc.script_comments.append(&mut comments);
      let tokens = to_estree_tokens_json(
        &ret.tokens,
        &ret.program,
        source,
        &Utf8ToUtf16::new(source),
        ESTreeTokenOptions::new(ret.program.source_type.is_typescript()),
      );

      let tokens = self.vue_allocator.alloc_str(&tokens[1..tokens.len() - 1]);
      Some(OxcParseReturn {
        directives: ret.program.directives,
        statements: ret.program.body,
        module_record: ret.module_record,
        references,
        tokens,
      })
    }
  }

  fn collect_semantic_references<'c>(&self, program: &Program<'c>) -> ArenaVec<'a, Reference<'a>> {
    let semantic = SemanticBuilder::new().build(program).semantic;
    let mut references = ArenaVec::new_in(self.vue_allocator);
    for reference_id in semantic.scoping().root_unresolved_references_ids().flatten() {
      let reference = semantic.scoping().get_reference(reference_id);
      let Some(identifier) =
        semantic.nodes().get_node(reference.node_id()).kind().as_identifier_reference()
      else {
        continue;
      };
      let name = self.vue_allocator.alloc_str(identifier.name.as_str());
      references.push(Reference {
        name,
        span: identifier.span,
        mode: reference_mode(reference.is_read(), reference.is_write()),
        has_variable: false,
      });
    }
    references
  }

  /// Reset the mutable source buffer to match the original source.
  ///
  /// Called after each in-place wrap-and-parse cycle (see the RFC's
  /// "Reusing the `oxc_parse` mutation trick" section).
  const fn sync_source_text(&mut self) {
    // SAFETY: `self.origin_source_text` and `self.mut_ptr_source_text` have
    // identical lengths; the former lives on the heap and the latter in the
    // arena, so the regions cannot overlap.
    unsafe {
      ptr::copy_nonoverlapping(
        self.origin_source_text.as_ptr(),
        self.mut_ptr_source_text.cast(),
        self.origin_source_text.len(),
      );
    }
  }
}

const fn reference_mode(is_read: bool, is_write: bool) -> &'static str {
  match (is_read, is_write) {
    (true, true) => "rw",
    (false, true) => "w",
    _ => "r",
  }
}
