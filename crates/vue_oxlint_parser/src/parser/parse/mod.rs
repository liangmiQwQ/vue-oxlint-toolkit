mod attributes;
mod children;
mod element;
mod expressions;
mod utils;
mod variables;

use crate::VueParser;
use crate::lexer::{Lexer, VToken, VTokenKind};

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
      self.parser.sfc.template_tokens.push(token.into());
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
}
