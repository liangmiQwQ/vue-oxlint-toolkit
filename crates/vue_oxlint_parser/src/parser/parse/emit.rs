use crate::VueParser;
use crate::ast::{VComment, token::SerializableToken};
use crate::lexer::{VToken, VTokenKind};
use crate::parser::parse::{token_end, token_start};
use oxc_span::Span;

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub(super) fn push_template_comment(&mut self, token: VToken<'b>) {
    let r#type =
      if token.kind == VTokenKind::HTMLComment { "HTMLComment" } else { "HTMLBogusComment" };
    self.sfc.template_comments.push(VComment {
      r#type,
      value: token.value.unwrap_or_default(),
      span: token.span,
    });
  }

  pub(super) fn push_template_oxc_tokens(&mut self, tokens: &'a str) {
    if !tokens.is_empty() {
      self.sfc.template_tokens.push(tokens.into());
    }
  }

  pub(super) fn push_manual_oxc_token(
    &mut self,
    token_type: &str,
    value: &str,
    start: usize,
    end: usize,
  ) {
    let token =
      format!(r#"{{"type":"{token_type}","value":"{value}","start":{start},"end":{end}}}"#);
    let token = self.vue_allocator.alloc_str(&token);
    self.sfc.template_tokens.push(token.into());
  }

  pub(super) fn push_script_punctuator(&mut self, start: usize, end: usize, value: &'static str) {
    self.sfc.script_tokens.push(
      VToken::new(VTokenKind::Punctuator, Span::new(start as u32, end as u32), Some(value)).into(),
    );
  }

  pub(super) fn push_template_vtoken(&mut self, token: VToken<'b>) {
    self.push_template_token(token.kind, token_start(token), token_end(token), token.value);
  }

  pub(super) fn push_template_token(
    &mut self,
    kind: VTokenKind,
    start: usize,
    end: usize,
    value: Option<&'b str>,
  ) {
    self.sfc.template_tokens.push(SerializableToken::from(VToken::new(
      kind,
      Span::new(start as u32, end as u32),
      value,
    )));
  }
}
