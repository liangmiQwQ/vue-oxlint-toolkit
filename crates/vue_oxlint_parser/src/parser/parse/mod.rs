mod attributes;
mod children;
mod element;
mod expressions;
mod utils;
mod variables;

use crate::VueParser;
use crate::lexer::{Lexer, VToken, VTokenKind};
use oxc_span::Span;

pub struct TemplateParser<'p, 'a, 'b>
where
  'b: 'a,
{
  parser: &'p mut VueParser<'a, 'b>,
  lexer: Lexer<'b>,
  peeked: Option<VToken<'b>>,
}

impl<'p, 'a, 'b> TemplateParser<'p, 'a, 'b>
where
  'b: 'a,
{
  pub const fn new(parser: &'p mut VueParser<'a, 'b>) -> Self {
    Self { lexer: Lexer::new(parser.source_text), parser, peeked: None }
  }

  pub fn parse(&mut self) -> bool {
    let children = self.parse_children(None);
    self.parser.sfc.children = children;
    self.lexer.panicked()
  }

  fn next_non_ws(&mut self) -> Option<VToken<'b>> {
    loop {
      let token = self.next()?;
      if token.kind != VTokenKind::HTMLWhitespace {
        return Some(token);
      }
    }
  }

  fn peek(&mut self) -> Option<VToken<'b>> {
    if self.peeked.is_none() {
      self.peeked = self.lexer.next_token();
    }
    self.peeked
  }

  fn next(&mut self) -> Option<VToken<'b>> {
    self.peeked.take().or_else(|| self.lexer.next_token())
  }

  fn alloc_value(&self, value: &str) -> &'a str {
    self.parser.vue_allocator.alloc_str(value)
  }

  fn push_template_token<'c>(&mut self, token: VToken<'c>)
  where
    'c: 'a,
  {
    self.parser.sfc.template_tokens.push(token.into());
  }

  fn push_template_token_with_value(&mut self, kind: VTokenKind, span: Span, value: &str) {
    let value = self.alloc_value(value);
    self.push_template_token(VToken::new(kind, span, Some(value)));
  }

  fn push_template_punctuator(&mut self, value: &str, span: Span) {
    self.push_template_token_with_value(VTokenKind::Punctuator, span, value);
  }

  fn push_script_wrapper(&mut self, value: &str, span: Span) {
    let token = format!(
      r#"{{"type":"Punctuator","value":"{value}","start":{},"end":{}}}"#,
      span.start, span.end,
    );
    let token = self.alloc_value(&token);
    self.parser.sfc.script_tokens.push(token.into());
  }

  fn push_quoted_expression_tokens(&mut self, outer_span: Span, tokens: &'a str) {
    self.push_opening_quote(outer_span);
    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }
    self.push_closing_quote(outer_span);
  }

  fn push_opening_quote(&mut self, outer_span: Span) {
    let source = outer_span.source_text(self.parser.source_text);
    let quoted = source
      .as_bytes()
      .first()
      .zip(source.as_bytes().last())
      .is_some_and(|(first, last)| matches!(*first, b'"' | b'\'') && first == last);

    if quoted {
      let quote = &self.parser.source_text[outer_span.start as usize..=outer_span.start as usize];
      self.push_template_punctuator(quote, Span::sized(outer_span.start, 1));
    }
  }

  fn push_closing_quote(&mut self, outer_span: Span) {
    let source = outer_span.source_text(self.parser.source_text);
    let quoted = source
      .as_bytes()
      .first()
      .zip(source.as_bytes().last())
      .is_some_and(|(first, last)| matches!(*first, b'"' | b'\'') && first == last);

    if quoted {
      let quote_start = outer_span.end - 1;
      let quote = &self.parser.source_text[quote_start as usize..outer_span.end as usize];
      self.push_template_punctuator(quote, Span::new(quote_start, outer_span.end));
    }
  }
}
