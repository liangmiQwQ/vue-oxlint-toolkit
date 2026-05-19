use crate::VueParser;
use crate::ast::bindings::ReferenceKind;
use crate::lexer::{VToken, VTokenKind};
use crate::parser::parse::state::{AttrValueKind, CurrentTag, TagAttribute, TagAttrs};
use crate::parser::parse::tree::{
  ParsedAttribute, ParsedDirective, ParsedDirectiveArgument, ParsedDirectiveKey,
  ParsedForDirective, ParsedIdentifier, ParsedLiteral, ParsedPureAttribute, ParsedSlotDirective,
};
use crate::parser::parse::{token_end, token_start};
use oxc_allocator::Box as ArenaBox;
use oxc_allocator::FromIn;
use oxc_ast::ast::{Expression, IdentifierReference};
use oxc_span::Span;
use oxc_str::Ident;
use oxc_syntax::node::NodeId;
use std::cell::Cell;

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
    {
      if tag.awaiting_attr_value.is_some() {
        self.extend_attr_value(tag, token);
        return;
      }

      if tag.attr_name_start.is_none() {
        tag.attr_name_start = Some(token_start(token));
      }
      tag.attr_name_end = token_end(token);
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
    if let Some(tag) = current_tag {
      if tag.awaiting_attr_value.is_some() {
        self.extend_attr_value(tag, token);
        return;
      }

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
      && let Some(pending_attr) = tag.awaiting_attr_value.take()
    {
      Self::push_attr_value(tag, pending_attr, token);
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
    tag.attributes.push(TagAttribute {
      name_start: start,
      name_end: end,
      name,
      association: None,
      value: None,
    });
    tag.attr_name_end = 0;
  }

  pub(super) fn start_attr_value(&self, tag: &mut CurrentTag<'b>, token: VToken<'b>) {
    let Some(start) = tag.attr_name_start.take() else {
      if let Some(mut pending_attr) = tag.attributes.pop() {
        pending_attr.association = Some(token);
        tag.awaiting_attr_value = Some(pending_attr);
      }
      return;
    };
    let end = tag.attr_name_end;
    let name = &self.source_text[start..end];
    tag.awaiting_attr_value = Some(TagAttribute {
      name_start: start,
      name_end: end,
      name,
      association: Some(token),
      value: None,
    });
    tag.attr_name_end = 0;
  }

  pub(super) fn flush_attr_value(&self, tag: &mut CurrentTag<'b>) {
    if let Some(mut pending_attr) = tag.awaiting_attr_value.take() {
      if let Some(value) = pending_attr.value {
        let start = token_start(value);
        let end = token_end(value);
        pending_attr.value = Some(VToken::new(
          VTokenKind::HTMLLiteral,
          value.span,
          Some(&self.source_text[start..end]),
        ));
      }
      tag.attributes.push(pending_attr);
    }
  }

  fn extend_attr_value(&self, tag: &mut CurrentTag<'b>, token: VToken<'b>) {
    if let Some(attr) = &mut tag.awaiting_attr_value {
      let start = attr.value.map_or(token.span.start, |value| value.span.start);
      attr.value = Some(VToken::new(
        VTokenKind::HTMLLiteral,
        oxc_span::Span::new(start, token.span.end),
        Some(&self.source_text[start as usize..token.span.end as usize]),
      ));
    }
  }

  fn push_attr_value(
    tag: &mut CurrentTag<'b>,
    mut pending_attr: TagAttribute<'b>,
    token: VToken<'b>,
  ) {
    pending_attr.value = Some(token);
    tag.attributes.push(pending_attr);
    tag.attr_name_start = None;
    tag.attr_name_end = 0;
  }

  pub(super) fn analyze_tag_attrs(tag: &mut CurrentTag<'b>) {
    tag.attrs = tag.attributes.iter().fold(TagAttrs::default(), |mut attrs, attr| {
      if attr.name.eq_ignore_ascii_case("setup") {
        attrs.setup = true;
      } else if attr.name.eq_ignore_ascii_case("v-pre") {
        attrs.v_pre = true;
      } else if attr.name.eq_ignore_ascii_case("lang")
        && let Some(value) = attr.value.and_then(|token| token.value)
      {
        attrs.lang = Some(value);
      }
      attrs
    });
  }

  pub(super) fn emit_tag_attrs(&mut self, tag: &CurrentTag<'b>) {
    for attr in &tag.attributes {
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

  pub(super) fn parse_tag_attributes(&mut self, tag: &CurrentTag<'b>) -> Vec<ParsedAttribute<'b>> {
    tag.attributes.iter().map(|attr| self.parse_tag_attribute(tag, *attr)).collect()
  }

  fn parse_tag_attribute(
    &mut self,
    tag: &CurrentTag<'b>,
    attr: TagAttribute<'b>,
  ) -> ParsedAttribute<'b> {
    if (tag.attrs.v_pre && !attr.name.eq_ignore_ascii_case("v-pre"))
      || !is_directive_attribute(attr.name)
    {
      return ParsedAttribute::Pure(Self::parse_pure_attribute(attr));
    }

    let key = self.parse_directive_key(attr.name, attr.name_start, attr.name_end);
    let span = Span::new(attr.name_start as u32, attr_span_end(attr) as u32);
    let kind = attr_value_kind(attr.name);

    if key.name.name == "slot" {
      let value = self.parse_slot_attr_value(attr);
      return ParsedAttribute::Slot(ParsedSlotDirective { key, value, span });
    }

    if key.name.name == "for" {
      let value = self.parse_v_for_attr_value(attr);
      return ParsedAttribute::For(ParsedForDirective { key, value, span });
    }

    let value = if matches!(kind, AttrValueKind::Literal) {
      None
    } else {
      self.parse_expression_attr_value(attr).or_else(|| self.parse_bind_shorthand_expression(&key))
    };

    ParsedAttribute::Directive(ParsedDirective { key, value, span })
  }

  fn parse_pure_attribute(attr: TagAttribute<'b>) -> ParsedPureAttribute<'b> {
    ParsedPureAttribute {
      key: ParsedIdentifier {
        name: attr.name,
        raw_name: attr.name,
        span: Span::new(attr.name_start as u32, attr.name_end as u32),
      },
      value: attr
        .value
        .map(|value| ParsedLiteral { value: value.value.unwrap_or_default(), span: value.span }),
      span: Span::new(attr.name_start as u32, attr_span_end(attr) as u32),
    }
  }

  fn parse_expression_attr_value(
    &mut self,
    attr: TagAttribute<'b>,
  ) -> Option<crate::parser::parse::tree::ParsedDirectiveExpression<'b>> {
    let value = attr.value?;
    let (start, end) = self.unquoted_value_span(value);
    if start >= end {
      return None;
    }

    self.parse_directive_expression_node(
      Span::new(start as u32, end as u32),
      Span::new(token_start(value) as u32, token_end(value) as u32),
    )
  }

  fn parse_slot_attr_value(
    &mut self,
    attr: TagAttribute<'b>,
  ) -> Option<crate::parser::parse::tree::ParsedSlotExpression<'b>> {
    let value = attr.value?;
    let (start, end) = self.unquoted_value_span(value);
    self.parse_slot_expression_node(
      start,
      end,
      Span::new(token_start(value) as u32, token_end(value) as u32),
    )
  }

  fn parse_v_for_attr_value(
    &mut self,
    attr: TagAttribute<'b>,
  ) -> Option<crate::parser::parse::tree::ParsedForExpression<'b>> {
    let value = attr.value?;
    let (start, end) = self.unquoted_value_span(value);
    self.parse_v_for_expression_node(
      start,
      end,
      Span::new(token_start(value) as u32, token_end(value) as u32),
    )
  }

  fn unquoted_value_span(&self, token: VToken<'b>) -> (usize, usize) {
    let start = token_start(token);
    let end = token_end(token);
    if start < end && matches!(self.byte(start), b'\'' | b'"') {
      (start + 1, end - 1)
    } else {
      (start, end)
    }
  }

  fn parse_directive_key(
    &mut self,
    name: &'b str,
    start: usize,
    end: usize,
  ) -> ParsedDirectiveKey<'b> {
    if let Some(prefix) = name.as_bytes().first().copied()
      && matches!(prefix, b':' | b'@' | b'#')
    {
      let semantic_name = match prefix {
        b':' => "bind",
        b'@' => "on",
        b'#' => "slot",
        _ => unreachable!(),
      };
      let semantic_name = self.alloc_str(semantic_name);
      let (argument, modifiers) = self.parse_argument_and_modifiers(start + 1, end);

      return ParsedDirectiveKey {
        name: ParsedIdentifier {
          name: semantic_name,
          raw_name: &self.source_text[start..=start],
          span: Span::new(start as u32, (start + 1) as u32),
        },
        argument,
        modifiers,
        span: Span::new(start as u32, end as u32),
      };
    }

    let name_start = start + 2;
    let directive_name_end = self.directive_name_end(name_start, end);
    let raw_name = &self.source_text[name_start..directive_name_end];
    let rest_start = if directive_name_end < end && self.byte(directive_name_end) == b':' {
      directive_name_end + 1
    } else {
      directive_name_end
    };
    let (argument, modifiers) = self.parse_argument_and_modifiers(rest_start, end);

    ParsedDirectiveKey {
      name: ParsedIdentifier {
        name: raw_name,
        raw_name,
        span: Span::new(start as u32, directive_name_end as u32),
      },
      argument,
      modifiers,
      span: Span::new(start as u32, end as u32),
    }
  }

  fn directive_name_end(&self, mut start: usize, end: usize) -> usize {
    while start < end && !matches!(self.byte(start), b':' | b'.') {
      start += 1;
    }
    start
  }

  fn parse_argument_and_modifiers(
    &mut self,
    start: usize,
    end: usize,
  ) -> (Option<ParsedDirectiveArgument<'b>>, Vec<ParsedIdentifier<'b>>) {
    if start >= end {
      return (None, Vec::new());
    }

    let (argument, modifier_start) = if self.byte(start) == b'[' {
      let Some(arg_end) = self.find_dynamic_arg_end(start, end) else {
        return (None, self.parse_modifiers(start, end));
      };
      {
        let expression = self.parse_directive_expression_node(
          Span::new((start + 1) as u32, (arg_end - 1) as u32),
          Span::new(start as u32, arg_end as u32),
        );
        (expression.map(ParsedDirectiveArgument::Dynamic), arg_end)
      }
    } else {
      let first_dot = self.source_text[start..end].find('.').map(|offset| start + offset);
      let arg_end = first_dot.unwrap_or(end);
      let argument = (start < arg_end).then(|| {
        ParsedDirectiveArgument::Static(ParsedIdentifier {
          name: &self.source_text[start..arg_end],
          raw_name: &self.source_text[start..arg_end],
          span: Span::new(start as u32, arg_end as u32),
        })
      });
      (argument, arg_end)
    };

    (argument, self.parse_modifiers(modifier_start, end))
  }

  fn parse_bind_shorthand_expression(
    &mut self,
    key: &ParsedDirectiveKey<'b>,
  ) -> Option<crate::parser::parse::tree::ParsedDirectiveExpression<'b>> {
    if key.name.name != "bind" || !key.modifiers.is_empty() {
      return None;
    }

    let Some(ParsedDirectiveArgument::Static(argument)) = key.argument else {
      return None;
    };

    if !argument.name.contains('-') {
      return self.parse_directive_expression_node(argument.span, argument.span).map(
        |mut expression| {
          expression.reference_kind = ReferenceKind::UnresolvedVariable;
          expression
        },
      );
    }

    let name = Self::camelize(argument.name);
    Some(crate::parser::parse::tree::ParsedDirectiveExpression {
      expression: Expression::Identifier(ArenaBox::new_in(
        IdentifierReference {
          node_id: Cell::new(NodeId::DUMMY),
          span: argument.span,
          name: Ident::from_in(name, self.js_allocator),
          reference_id: Cell::new(None),
        },
        self.js_allocator,
      )),
      reference_kind: ReferenceKind::UnresolvedVariable,
      span: argument.span,
    })
  }

  fn camelize(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut uppercase_next = false;
    for char in value.chars() {
      if char == '-' {
        uppercase_next = true;
      } else if uppercase_next {
        result.extend(char.to_uppercase());
        uppercase_next = false;
      } else {
        result.push(char);
      }
    }
    result
  }

  fn parse_modifiers(&self, mut start: usize, end: usize) -> Vec<ParsedIdentifier<'b>> {
    let mut modifiers = Vec::new();
    while start < end {
      if self.byte(start) != b'.' {
        break;
      }

      let modifier_start = start + 1;
      let next_dot =
        self.source_text[modifier_start..end].find('.').map(|offset| modifier_start + offset);
      let modifier_end = next_dot.unwrap_or(end);
      if modifier_start < modifier_end {
        let modifier = &self.source_text[modifier_start..modifier_end];
        modifiers.push(ParsedIdentifier {
          name: modifier,
          raw_name: modifier,
          span: Span::new(modifier_start as u32, modifier_end as u32),
        });
      }
      start = modifier_end;
    }
    modifiers
  }

  fn emit_attr_name(&mut self, name: &'b str, start: usize, end: usize) {
    if let Some(first) = name.as_bytes().first().copied()
      && matches!(first, b':' | b'@' | b'#')
      && name.len() > 1
    {
      self.push_template_token(VTokenKind::Punctuator, start, start + 1, Some(&name[..1]));
      self.emit_arg_and_modifiers(start + 1, end);
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
      self.emit_arg_and_modifiers(colon + 1, end);
      return;
    }

    self.emit_static_arg_and_modifiers(start, end);
  }

  fn emit_arg_and_modifiers(&mut self, start: usize, end: usize) {
    if start < end
      && self.byte(start) == b'['
      && let Some(arg_end) = self.find_dynamic_arg_end(start, end)
    {
      self.push_template_token(VTokenKind::Punctuator, start, start + 1, Some("["));
      self.emit_expression_tokens(start + 1, arg_end - 1);
      self.push_template_token(VTokenKind::Punctuator, arg_end - 1, arg_end, Some("]"));
      self.emit_modifiers(arg_end, end);
    } else if start < end {
      self.emit_static_arg_and_modifiers(start, end);
    }
  }

  fn emit_static_arg_and_modifiers(&mut self, start: usize, end: usize) {
    let first_dot = self.source_text[start..end].find('.').map(|offset| start + offset);
    let arg_end = first_dot.unwrap_or(end);
    if start < arg_end {
      self.push_template_token(
        VTokenKind::HTMLIdentifier,
        start,
        arg_end,
        Some(&self.source_text[start..arg_end]),
      );
    }
    self.emit_modifiers(arg_end, end);
  }

  fn emit_modifiers(&mut self, mut start: usize, end: usize) {
    while start < end {
      if self.byte(start) != b'.' {
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          start,
          end,
          Some(&self.source_text[start..end]),
        );
        return;
      }

      self.push_template_token(VTokenKind::Punctuator, start, start + 1, Some("."));
      let modifier_start = start + 1;
      let next_dot =
        self.source_text[modifier_start..end].find('.').map(|offset| modifier_start + offset);
      let modifier_end = next_dot.unwrap_or(end);
      if modifier_start < modifier_end {
        self.push_template_token(
          VTokenKind::HTMLIdentifier,
          modifier_start,
          modifier_end,
          Some(&self.source_text[modifier_start..modifier_end]),
        );
      }
      start = modifier_end;
    }
  }

  fn find_dynamic_arg_end(&self, start: usize, end: usize) -> Option<usize> {
    let mut pos = start;
    let mut depth = 0usize;
    let mut quote = None;

    while pos < end {
      let byte = self.byte(pos);
      if let Some(current_quote) = quote {
        if byte == current_quote {
          quote = None;
        } else if byte == b'\\' {
          pos += 1;
        }
      } else if matches!(byte, b'\'' | b'"' | b'`') {
        quote = Some(byte);
      } else if byte == b'[' {
        depth += 1;
      } else if byte == b']' {
        depth = depth.saturating_sub(1);
        if depth == 0 {
          return Some(pos + 1);
        }
      }
      pos += 1;
    }
    None
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
  if attr_name == "v-for" || attr_name.starts_with("v-for.") {
    return AttrValueKind::VFor;
  }

  if attr_name == "v-on" || attr_name.starts_with("v-on:") {
    return AttrValueKind::Handler;
  }

  if attr_name.as_bytes().first() == Some(&b'@') && attr_name.len() > 1 {
    return AttrValueKind::Handler;
  }

  if (attr_name.as_bytes().first() == Some(&b'#') && attr_name.len() > 1)
    || attr_name == "v-slot"
    || (attr_name.starts_with("v-slot:") && !attr_name.ends_with(':'))
    || attr_name.starts_with("v-slot.")
  {
    return AttrValueKind::SlotParams;
  }

  if (attr_name.as_bytes().first() == Some(&b':') && attr_name.len() > 1)
    || attr_name == "v-bind"
    || matches!(attr_name, "v-if" | "v-else-if" | "v-model")
    || attr_name.starts_with("v-bind:")
    || attr_name.starts_with("v-bind.")
    || attr_name.starts_with("v-if.")
    || attr_name.starts_with("v-else-if.")
    || attr_name.starts_with("v-model:")
    || attr_name.starts_with("v-model.")
  {
    return AttrValueKind::Expression;
  }

  AttrValueKind::Literal
}

fn is_directive_attribute(name: &str) -> bool {
  if let Some(first) = name.as_bytes().first().copied()
    && matches!(first, b':' | b'@' | b'#')
  {
    return name.len() > 1;
  }

  name.starts_with("v-") && !name.ends_with(':') && name.len() > 2
}

const fn attr_span_end(attr: TagAttribute<'_>) -> usize {
  if let Some(value) = attr.value { value.span.end as usize } else { attr.name_end }
}
