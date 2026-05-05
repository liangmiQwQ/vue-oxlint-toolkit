mod irregular_whitespaces;
mod module_record;
mod oxc_parse;
mod parse;

use crate::parser::irregular_whitespaces::collect_irregular_whitespaces;
use crate::parser::parse::TemplateParser;
use crate::{VueParser, VueParserReturn};

impl<'a, 'b> VueParser<'a, 'b> {
  #[must_use]
  pub fn parse(mut self) -> VueParserReturn<'a, 'b> {
    let mut template_parser = TemplateParser::new(&mut self);
    let panicked = template_parser.parse();

    self.fix_module_records();

    let Self { sfc, errors, clean_spans, module_record, source_text, .. } = self;

    VueParserReturn {
      sfc,
      irregular_whitespaces: collect_irregular_whitespaces(source_text),
      module_record,
      clean_spans,
      errors,
      panicked,
    }
  }
}
