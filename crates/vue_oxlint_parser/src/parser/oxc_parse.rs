use std::ptr;

use memchr::memmem::{find, rfind};
use oxc_allocator::{Allocator, CloneIn, TakeIn, Vec as ArenaVec};
use oxc_ast::ast::{Directive, Expression, Statement};
use oxc_ast_visit::utf8_to_utf16::Utf8ToUtf16;
use oxc_estree_tokens::{ESTreeTokenOptions, to_estree_tokens_json};
use oxc_parser::config::TokensParserConfig;
use oxc_span::Span;
use oxc_syntax::module_record::ModuleRecord;

use crate::VueParser;

#[allow(dead_code)]
impl<'a, 'b, 'c> VueParser<'a, 'b>
where
  'b: 'a,
  'a: 'c,
{
  pub(crate) fn parse_pure_expression(&mut self, span: Span) -> Option<(Expression<'b>, &str)> {
    let allocator = Allocator::new();
    // SAFETY: use `()` as wrap
    let (expr, tokens) = unsafe { self.parse_expression(span, b"(", b")", &allocator) }?;
    Some((expr.clone_in(self.js_allocator), tokens))
  }

  /// Parse expression with [`oxc_parser`]
  /// The reason we don't wrap the expression with `(` and `)` is to avoid unnecessary copy
  /// `b"(("` and `b"))=>{})"` is much more efficient than passing `b"("` `b")=>{}"` and copy it in a [`Vec`] and push and slice
  ///
  /// ## Safety
  /// - `start_wrap` must start with `(`
  /// - `end_wrap` must end with `)`
  pub(crate) unsafe fn parse_expression(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: &'c Allocator,
  ) -> Option<(Expression<'c>, &'a str)> {
    // The only purpose to not use [`oxc_parser::Parser::parse_expression`] is to keep the code comments in it
    let (_, mut body, _, tokens) = self.oxc_parse(span, start_wrap, end_wrap, Some(allocator))?;

    let Some(Statement::ExpressionStatement(stmt)) = body.get_mut(0) else {
      // SAFETY: We always wrap the source in parentheses, so it should always be an expression statement.
      unreachable!()
    };
    let Expression::ParenthesizedExpression(expression) = &mut stmt.expression else {
      // SAFETY: We always wrap the source in parentheses, so it should always be a parenthesized expression
      unreachable!()
    };

    // it mustn't be the first or last element in the whole array.
    let tokens = tokens.as_bytes();
    // SAFETY: we add "start_wrap" and "end_wrap", so there must be a token which contains corresponding "end" field
    let start_needle = format!(r#""end":{}}},"#, span.start - 1);
    let start = find(tokens, start_needle.as_bytes()).unwrap() + start_needle.len();
    let end_needle = format!(r#""end":{}}}"#, span.end);
    let end = rfind(tokens, end_needle.as_bytes()).unwrap() + end_needle.len();

    Some((expression.expression.take_in(self.js_allocator), unsafe {
      // SAFETY: it is sliced from a &str
      str::from_utf8_unchecked(&tokens[start..end])
    }))
  }

  /// Call [`oxc_parser::Parser::parse`] with a custom wrap
  /// Everything before `start` and `start_wrap` will be ignored
  ///
  /// If you need to parse with any wrapper, it will produce unused AST nodes
  /// `allocator` param in `'c` lifetime should provided and drop unused AST nodes as a temporary Arena
  pub(crate) fn oxc_parse(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: Option<&'c Allocator>,
  ) -> Option<(ArenaVec<'c, Directive<'c>>, ArenaVec<'c, Statement<'c>>, ModuleRecord<'c>, &'a str)>
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

  fn call_oxc_parse(
    &mut self,
    source: &'a str,
    allocator: &'c Allocator,
  ) -> Option<(ArenaVec<'c, Directive<'c>>, ArenaVec<'c, Statement<'c>>, ModuleRecord<'c>, &'a str)>
  {
    // SAFETY: all oxc_parse happens after <script> tag parsing
    let mut ret = oxc_parser::Parser::new(allocator, source, self.sfc.source_type.unwrap())
      .with_options(self.options)
      .with_config(TokensParserConfig)
      .parse();

    self.errors.append(&mut ret.errors);
    if ret.panicked {
      None
    } else {
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
      Some((ret.program.directives, ret.program.body, ret.module_record, &tokens))
    }
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
