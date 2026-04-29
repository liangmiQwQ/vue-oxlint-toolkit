use oxc_ast::ast::JSXChild;
use oxc_span::Span;

use crate::parser::ParserImpl;

impl<'a> ParserImpl<'a> {
  #[inline]
  pub fn jsx_child_text(&self, span: Span, str: &str) -> JSXChild<'a> {
    let ast_str = if true {
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
}
