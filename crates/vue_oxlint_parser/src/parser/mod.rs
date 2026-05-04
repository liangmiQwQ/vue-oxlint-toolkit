mod irregular_whitespaces;
mod module_record;
mod oxc_parse;

use crate::lexer::Lexer;
use crate::parser::irregular_whitespaces::collect_irregular_whitespaces;
use crate::{VueParser, VueParserReturn};

impl<'a, 'b> VueParser<'a, 'b> {
  #[must_use]
  pub fn parse(self) -> VueParserReturn<'a, 'b> {
    let mut lexer = Lexer::new(self.vue_allocator, self.source_text);

    while let Some(token) = lexer.next_token() {
      println!("token: {token:#?}");
      // TODO: Should move to `parser/parse` module (divide logic into different modules (files))
    }

    let Self { sfc, errors, clean_spans, module_record, source_text, .. } = self;

    VueParserReturn {
      sfc,
      irregular_whitespaces: collect_irregular_whitespaces(source_text),
      module_record,
      clean_spans,
      errors,
      panicked: false,
    }
  }
}
