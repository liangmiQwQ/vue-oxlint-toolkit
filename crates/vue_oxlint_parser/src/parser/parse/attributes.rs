use crate::VueParser;
use crate::lexer::{VToken, VTokenKind};
use crate::parser::parse::state::{AttrValueKind, CurrentTag};
use crate::parser::parse::{token_end, token_start};

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub(super) fn handle_identifier(
    &mut self,
    token: VToken<'b>,
    current_tag: &mut Option<CurrentTag<'b>>,
  ) {
    if let Some(tag) = current_tag
      && !tag.is_end
      && let Some(value) = token.value
    {
      if tag.awaiting_attr_value.is_none() {
        if tag.attr_name_start.is_none() {
          tag.attr_name_start = Some(token_start(token));
        }
        tag.attr_name_end = token_end(token);
      }
      tag.last_attr_name = Some(value);
      if value.eq_ignore_ascii_case("setup") {
        tag.attrs.setup = true;
      } else if value.eq_ignore_ascii_case("v-pre") {
        tag.attrs.v_pre = true;
      }
    }

    if current_tag.is_none() {
      self.push_template_vtoken(token);
    }
  }

  pub(super) fn handle_attr_name_part(
    &mut self,
    token: VToken<'b>,
    current_tag: &mut Option<CurrentTag<'b>>,
  ) {
    if let Some(tag) = current_tag
      && tag.awaiting_attr_value.is_none()
    {
      if tag.attr_name_start.is_none() {
        tag.attr_name_start = Some(token_start(token));
      }
      tag.attr_name_end = token_end(token);
    }

    if current_tag.is_none() {
      self.push_template_vtoken(token);
    }
  }

  pub(super) fn handle_literal(
    &mut self,
    token: VToken<'b>,
    current_tag: &mut Option<CurrentTag<'b>>,
  ) {
    let mut is_v_pre_attr = false;
    let literal_attr_name = if let Some(tag) = current_tag
      && let Some(current_attr_name) = tag.awaiting_attr_value.take()
    {
      is_v_pre_attr = tag.attrs.v_pre;
      if current_attr_name.eq_ignore_ascii_case("lang") {
        tag.attrs.lang = token.value;
      }
      tag.attr_name_start = None;
      tag.attr_name_end = 0;
      tag.last_attr_name = None;
      tag.flushed_attr_name = None;
      Some(current_attr_name)
    } else {
      None
    };

    if let Some(attr_name) = literal_attr_name {
      self.emit_attr_value(attr_name, token, is_v_pre_attr);
    } else {
      self.push_template_vtoken(token);
    }
  }

  pub(super) fn flush_attr_name(&mut self, tag: &mut CurrentTag<'b>) {
    let Some(start) = tag.attr_name_start.take() else {
      return;
    };
    let end = tag.attr_name_end;
    let name = &self.source_text[start..end];
    self.emit_attr_name(name, start, end);
    tag.flushed_attr_name = Some(name);
    tag.attr_name_end = 0;
  }

  fn emit_attr_name(&mut self, name: &'b str, start: usize, end: usize) {
    if let Some(first) = name.as_bytes().first().copied()
      && matches!(first, b':' | b'@' | b'#')
      && name.len() > 1
    {
      self.push_template_token(VTokenKind::Punctuator, start, start + 1, Some(&name[..1]));
      self.emit_dynamic_arg_tokens(start + 1, end);
      return;
    }

    if let Some(colon_offset) = name.find(':')
      && !name.ends_with(':')
    {
      let colon = start + colon_offset;
      self.push_template_token(
        VTokenKind::HTMLIdentifier,
        start,
        colon,
        Some(&self.source_text[start..colon]),
      );
      self.push_template_token(VTokenKind::Punctuator, colon, colon + 1, Some(":"));
      self.emit_dynamic_arg_tokens(colon + 1, end);
      return;
    }

    if let Some(dot_offset) = name.find('.') {
      let dot = start + dot_offset;
      self.push_template_token(
        VTokenKind::HTMLIdentifier,
        start,
        dot,
        Some(&self.source_text[start..dot]),
      );
      self.push_template_token(VTokenKind::Punctuator, dot, dot + 1, Some("."));
      self.push_template_token(
        VTokenKind::HTMLIdentifier,
        dot + 1,
        end,
        Some(&self.source_text[dot + 1..end]),
      );
      return;
    }

    self.push_template_token(VTokenKind::HTMLIdentifier, start, end, Some(name));
  }

  fn emit_dynamic_arg_tokens(&mut self, start: usize, end: usize) {
    if start < end && self.byte(start) == b'[' && self.byte(end - 1) == b']' {
      self.push_template_token(VTokenKind::Punctuator, start, start + 1, Some("["));
      self.emit_expression_tokens(start + 1, end - 1);
      self.push_template_token(VTokenKind::Punctuator, end - 1, end, Some("]"));
    } else if start < end {
      if let Some(dot_offset) = self.source_text[start..end].find('.') {
        let dot = start + dot_offset;
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          start,
          dot,
          Some(&self.source_text[start..dot]),
        );
        self.push_template_token(VTokenKind::Punctuator, dot, dot + 1, Some("."));
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          dot + 1,
          end,
          Some(&self.source_text[dot + 1..end]),
        );
      } else {
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          start,
          end,
          Some(&self.source_text[start..end]),
        );
      }
    }
  }

  fn emit_attr_value(&mut self, attr_name: &'b str, token: VToken<'b>, is_v_pre_attr: bool) {
    let Some(value) = token.value else {
      return;
    };

    let token_start = token_start(token);
    let token_end = token_end(token);
    let kind = attr_value_kind(attr_name);
    if is_v_pre_attr || matches!(kind, AttrValueKind::Literal) {
      self.push_template_token(VTokenKind::HTMLLiteral, token_start, token_end, Some(value));
      return;
    }

    let quoted = matches!(self.byte(token_start), b'\'' | b'"');
    let value_start = if quoted { token_start + 1 } else { token_start };
    let value_end = if quoted { token_end - 1 } else { token_end };
    if quoted {
      self.push_template_token(
        VTokenKind::Punctuator,
        token_start,
        value_start,
        Some(&self.source_text[token_start..value_start]),
      );
    }

    match kind {
      AttrValueKind::Expression => self.emit_expression_tokens(value_start, value_end),
      AttrValueKind::Handler => self.emit_handler_tokens(value_start, value_end),
      AttrValueKind::SlotParams => self.emit_slot_params_tokens(value_start, value_end),
      AttrValueKind::VFor => self.emit_v_for_tokens(value_start, value_end),
      AttrValueKind::Literal => {}
    }

    if quoted {
      self.push_template_token(
        VTokenKind::Punctuator,
        value_end,
        token_end,
        Some(&self.source_text[value_end..token_end]),
      );
    }
  }
}

fn attr_value_kind(attr_name: &str) -> AttrValueKind {
  if attr_name == "v-for" {
    return AttrValueKind::VFor;
  }

  if attr_name == "v-on" || attr_name.starts_with("v-on:") {
    return AttrValueKind::Handler;
  }

  if attr_name.as_bytes().first() == Some(&b'@') && attr_name.len() > 1 {
    return AttrValueKind::Handler;
  }

  if (attr_name.as_bytes().first() == Some(&b'#') && attr_name.len() > 1)
    || (attr_name.starts_with("v-slot:") && !attr_name.ends_with(':'))
  {
    return AttrValueKind::SlotParams;
  }

  if (attr_name.as_bytes().first() == Some(&b':') && attr_name.len() > 1)
    || attr_name == "v-bind"
    || matches!(attr_name, "v-if" | "v-else-if" | "v-model")
    || attr_name.starts_with("v-bind:")
  {
    return AttrValueKind::Expression;
  }

  AttrValueKind::Literal
}
