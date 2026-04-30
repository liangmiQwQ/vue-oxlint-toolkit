use oxc_ast::ast::Str;

use crate::parser::ParserImpl;

impl<'a> ParserImpl<'a> {
  pub(super) fn codegen_directive_identifier(&self, name: &'a str) -> Str<'a> {
    if !self.config.codegen || is_codegen_safe_jsx_identifier(name) {
      return name.into();
    }

    let mut result = String::from("__v_");
    for ch in name.chars() {
      if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '-') {
        result.push(ch);
      } else if !result.ends_with('_') {
        result.push('_');
      }
    }

    result.push_str("__");
    self.ast.str(&result)
  }
}

fn is_codegen_safe_jsx_identifier(name: &str) -> bool {
  let Some((&first, rest)) = name.as_bytes().split_first() else {
    return false;
  };

  matches!(first, b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$')
    && rest.iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'-'))
}
