use crate::VueParser;
use crate::lexer::{VToken, VTokenKind};
use crate::parser::parse::state::{AttrValueKind, CurrentTag, PendingAttr};
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
    let has_current_tag = current_tag.is_some();
    if let Some(tag) = current_tag
      && let Some(mut pending_attr) = tag.awaiting_attr_value.take()
    {
      if pending_attr.name.eq_ignore_ascii_case("lang") {
        tag.attrs.lang = token.value;
      }
      pending_attr.value = Some(token);
      tag.pending_attrs.push(pending_attr);
      tag.attr_name_start = None;
      tag.attr_name_end = 0;
      tag.last_attr_name = None;
      return;
    }

    if !has_current_tag {
      self.push_template_vtoken(token);
    }
  }

  pub(super) fn flush_attr_name(&self, tag: &mut CurrentTag<'b>) {
    let Some(start) = tag.attr_name_start.take() else {
      return;
    };
    let end = tag.attr_name_end;
    let name = &self.source_text[start..end];
    tag.pending_attrs.push(PendingAttr {
      name_start: start,
      name_end: end,
      name,
      association: None,
      value: None,
    });
    tag.attr_name_end = 0;
    tag.last_attr_name = None;
  }

  pub(super) fn start_attr_value(&self, tag: &mut CurrentTag<'b>, token: VToken<'b>) {
    let Some(start) = tag.attr_name_start.take() else {
      if let Some(mut pending_attr) = tag.pending_attrs.pop() {
        pending_attr.association = Some(token);
        tag.awaiting_attr_value = Some(pending_attr);
      }
      return;
    };
    let end = tag.attr_name_end;
    let name = &self.source_text[start..end];
    tag.awaiting_attr_value = Some(PendingAttr {
      name_start: start,
      name_end: end,
      name,
      association: Some(token),
      value: None,
    });
    tag.attr_name_end = 0;
    tag.last_attr_name = None;
  }

  pub(super) fn flush_attr_value(tag: &mut CurrentTag<'b>) {
    if let Some(pending_attr) = tag.awaiting_attr_value.take() {
      tag.pending_attrs.push(pending_attr);
    }
  }

  pub(super) fn emit_tag_attrs(&mut self, tag: &CurrentTag<'b>) {
    for attr in &tag.pending_attrs {
      if tag.attrs.v_pre {
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          attr.name_start,
          attr.name_end,
          Some(attr.name),
        );
      } else {
        self.emit_attr_name(attr.name, attr.name_start, attr.name_end);
      }

      if let Some(association) = attr.association {
        self.push_template_vtoken(association);
      }

      if let Some(value) = attr.value {
        self.emit_attr_value(attr.name, value, tag.attrs.v_pre);
      }
    }
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
