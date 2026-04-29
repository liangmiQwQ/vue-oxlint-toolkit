use oxc_ast::ast::{JSXChild, Str};
use oxc_span::Span;

use crate::parser::ParserImpl;

impl<'a> ParserImpl<'a> {
  #[inline]
  pub fn jsx_child_text(&self, span: Span, str: &str) -> JSXChild<'a> {
    let ast_str = if self.config.codegen {
      let bytes = str.as_bytes();
      let mut vec: Vec<u8> = Vec::with_capacity(bytes.len());
      for &b in bytes {
        match b {
          b'&' => vec.extend_from_slice(b"&amp;"),
          b'<' => vec.extend_from_slice(b"&lt;"),
          b'>' => vec.extend_from_slice(b"&gt;"),
          b'{' => vec.extend_from_slice(b"&#123;"),
          b'}' => vec.extend_from_slice(b"&#125;"),
          _ => vec.push(b),
        }
      }
      let escaped = unsafe { str::from_utf8_unchecked(&vec) };
      self.ast.str(escaped)
    } else {
      self.ast.str(str)
    };

    self.ast.jsx_child_text(span, ast_str, Some(ast_str))
  }

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
