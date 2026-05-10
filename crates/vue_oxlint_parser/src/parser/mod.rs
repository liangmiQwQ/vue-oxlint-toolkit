mod irregular_whitespaces;
mod module_record;
mod oxc_parse;

use crate::ast::VComment;
use crate::ast::token::SerializableToken;
use crate::lexer::{Lexer, LexerMode, VToken, VTokenKind};
use crate::parser::irregular_whitespaces::collect_irregular_whitespaces;
use crate::{VueParser, VueParserReturn};
use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, SourceType, Span};

#[derive(Debug, Default, Clone, Copy)]
struct TagAttrs<'s> {
  setup: bool,
  v_pre: bool,
  lang: Option<&'s str>,
}

#[derive(Debug)]
struct CurrentTag<'s> {
  name: &'s str,
  normalized_name: String,
  open_start: usize,
  is_end: bool,
  attrs: TagAttrs<'s>,
  last_attr_name: Option<&'s str>,
  attr_name_start: Option<usize>,
  attr_name_end: usize,
  flushed_attr_name: Option<&'s str>,
  awaiting_attr_value: Option<&'s str>,
}

#[derive(Debug, Clone)]
struct ElementState {
  name: String,
  v_pre: bool,
}

#[derive(Debug, Clone, Copy)]
struct ScriptInfo<'s> {
  open_start: usize,
  open_end: usize,
  attrs: TagAttrs<'s>,
}

#[derive(Debug, Clone)]
struct RawElement<'s> {
  body_start: usize,
  script: Option<ScriptInfo<'s>>,
}

#[derive(Debug, Clone)]
struct PendingScript<'s> {
  info: ScriptInfo<'s>,
  body_start: usize,
  body_end: usize,
}

#[derive(Debug, Clone, Copy)]
enum AttrValueKind {
  Literal,
  Expression,
  Handler,
  SlotParams,
  VFor,
}

#[derive(Debug, Clone, Copy)]
struct VForParts<'s> {
  left_start: usize,
  left_end: usize,
  operator: &'s str,
  operator_start: usize,
  operator_end: usize,
  right_start: usize,
  right_end: usize,
}

impl<'a, 'b> VueParser<'a, 'b> {
  #[must_use]
  pub fn parse(mut self) -> VueParserReturn<'a, 'b> {
    self.parse_tokens();

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

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  fn parse_tokens(&mut self) {
    self.sfc.source_type = Some(SourceType::mjs());

    let mut lexer = Lexer::new(self.js_allocator, self.source_text);
    let mut current_tag = None;
    let mut element_stack = Vec::new();
    let mut raw_element = None;
    let mut pending_script = None;
    let mut interpolation_start = None;
    let mut seen_script = false;
    let mut seen_setup = false;

    while let Some(token) = lexer.next_token() {
      if let Some(start) = interpolation_start {
        if token.kind == VTokenKind::VExpressionEnd {
          self.emit_expression_tokens(start, token_start(token));
          self.push_template_vtoken(token);
          interpolation_start = None;
        }
        continue;
      }

      match token.kind {
        VTokenKind::HTMLComment | VTokenKind::HTMLBogusComment => {
          self.push_template_comment(token);
        }
        VTokenKind::HTMLTagOpen | VTokenKind::HTMLEndTagOpen => {
          self.handle_tag_open(token, &mut current_tag, &mut raw_element, &mut pending_script);
        }
        VTokenKind::HTMLTagClose | VTokenKind::HTMLSelfClosingTagClose => {
          if let Some(tag) = &mut current_tag {
            self.flush_attr_name(tag);
          }
          self.push_template_vtoken(token);
          self.finish_tag(
            token,
            &mut lexer,
            &mut current_tag,
            &mut element_stack,
            &mut raw_element,
            &mut pending_script,
            &mut seen_script,
            &mut seen_setup,
          );
        }
        VTokenKind::HTMLIdentifier => {
          self.handle_identifier(token, &mut current_tag);
        }
        VTokenKind::Punctuator if current_tag.as_ref().is_some_and(|tag| !tag.is_end) => {
          self.handle_attr_name_part(token, &mut current_tag);
        }
        VTokenKind::HTMLAssociation => {
          if let Some(tag) = &mut current_tag {
            self.flush_attr_name(tag);
            tag.awaiting_attr_value = tag.flushed_attr_name.or(tag.last_attr_name);
          }
          self.push_template_vtoken(token);
        }
        VTokenKind::HTMLLiteral => {
          self.handle_literal(token, &mut current_tag);
        }
        VTokenKind::VExpressionStart => {
          self.push_template_vtoken(token);
          interpolation_start = Some(token_end(token));
        }
        VTokenKind::HTMLWhitespace if current_tag.is_some() => {
          if let Some(tag) = &mut current_tag
            && tag.awaiting_attr_value.is_none()
          {
            self.flush_attr_name(tag);
          }
        }
        _ => self.push_template_vtoken(token),
      }
    }
  }

  fn handle_tag_open(
    &mut self,
    token: VToken<'b>,
    current_tag: &mut Option<CurrentTag<'b>>,
    raw_element: &mut Option<RawElement<'b>>,
    pending_script: &mut Option<PendingScript<'b>>,
  ) {
    if let Some(raw) = raw_element.take() {
      let body_end = token.span.start as usize;
      if let Some(script) = raw.script {
        *pending_script =
          Some(PendingScript { info: script, body_start: raw.body_start, body_end });
      }
    }

    let name = token.value.unwrap_or_default();
    let normalized_name = name.to_ascii_lowercase();
    let value = self.alloc_str(&normalized_name);
    self.push_template_token(token.kind, token_start(token), token_end(token), Some(value));
    *current_tag = Some(CurrentTag {
      name,
      normalized_name,
      open_start: token_start(token),
      is_end: token.kind == VTokenKind::HTMLEndTagOpen,
      attrs: TagAttrs::default(),
      last_attr_name: None,
      attr_name_start: None,
      attr_name_end: 0,
      flushed_attr_name: None,
      awaiting_attr_value: None,
    });
  }

  fn handle_identifier(&mut self, token: VToken<'b>, current_tag: &mut Option<CurrentTag<'b>>) {
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

  fn handle_attr_name_part(&mut self, token: VToken<'b>, current_tag: &mut Option<CurrentTag<'b>>) {
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

  fn handle_literal(&mut self, token: VToken<'b>, current_tag: &mut Option<CurrentTag<'b>>) {
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

  fn flush_attr_name(&mut self, tag: &mut CurrentTag<'b>) {
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

    if let Some(colon) = name.find(':').filter(|_| !name.ends_with(':')) {
      let colon = start + colon;
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

    if let Some(dot) = name.find('.') {
      let dot = start + dot;
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
      if let Some(dot) = self.source_text[start..end].find('.') {
        let dot = start + dot;
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

  #[allow(clippy::too_many_arguments)]
  fn finish_tag(
    &mut self,
    token: VToken<'b>,
    lexer: &mut Lexer<'b>,
    current_tag: &mut Option<CurrentTag<'b>>,
    element_stack: &mut Vec<ElementState>,
    raw_element: &mut Option<RawElement<'b>>,
    pending_script: &mut Option<PendingScript<'b>>,
    seen_script: &mut bool,
    seen_setup: &mut bool,
  ) {
    let Some(tag) = current_tag.take() else {
      return;
    };

    let tag_end = token_end(token);
    if tag.is_end {
      pop_element(element_stack, &tag.normalized_name);
      if let Some(script) = pending_script.take() {
        self.emit_script_tokens(
          script.info,
          script.body_start,
          script.body_end,
          tag_end,
          seen_script,
          seen_setup,
        );
      }
      update_lexer_mode(lexer, element_stack);
      return;
    }

    if token.kind == VTokenKind::HTMLSelfClosingTagClose {
      update_lexer_mode(lexer, element_stack);
      return;
    }

    let state = ElementState { name: tag.normalized_name.clone(), v_pre: tag.attrs.v_pre };
    element_stack.push(state);

    if is_raw_text_element(&tag.normalized_name) || is_rcdata_element(&tag.normalized_name) {
      let end_tag = self.alloc_str(&format!("</{}", tag.normalized_name));
      let script = if tag.normalized_name == "script" {
        Some(ScriptInfo { open_start: tag.open_start, open_end: tag_end, attrs: tag.attrs })
      } else {
        None
      };
      *raw_element = Some(RawElement { body_start: tag_end, script });
      let mode = if is_raw_text_element(&tag.name.to_ascii_lowercase()) {
        LexerMode::RawText
      } else {
        LexerMode::RcData
      };
      lexer.set_mode_until(mode, end_tag);
      return;
    }

    update_lexer_mode(lexer, element_stack);
  }

  fn emit_script_tokens(
    &mut self,
    script: ScriptInfo<'b>,
    body_start: usize,
    body_end: usize,
    close_end: usize,
    seen_script: &mut bool,
    seen_setup: &mut bool,
  ) {
    if script.attrs.setup {
      if *seen_setup || body_start == body_end {
        *seen_setup = true;
        return;
      }
      *seen_setup = true;
    } else {
      if *seen_script || (*seen_setup && body_start == body_end) {
        return;
      }
      *seen_script = true;
    }

    self.push_script_punctuator(script.open_start, script.open_end, "<script>");
    self.apply_script_source_type(script.attrs.lang);

    if body_start < body_end {
      let span = Span::new(body_start as u32, body_end as u32);
      if let Some((_, _, _, tokens)) = self.oxc_parse(span, &[], &[], None)
        && !tokens.is_empty()
      {
        self.sfc.script_tokens.push(tokens.into());
      }
    }

    self.push_script_punctuator(body_end, close_end, "</script>");
  }

  fn apply_script_source_type(&mut self, lang: Option<&str>) {
    let source_type = if let Some(lang) = lang
      && let Ok(parsed) = SourceType::from_extension(lang)
    {
      parsed.with_module(true)
    } else {
      SourceType::mjs()
    };
    self.sfc.source_type = Some(source_type);
  }

  fn push_template_comment(&mut self, token: VToken<'b>) {
    let r#type =
      if token.kind == VTokenKind::HTMLComment { "HTMLComment" } else { "HTMLBogusComment" };
    self.sfc.template_comments.push(VComment {
      r#type,
      value: token.value.unwrap_or_default(),
      span: token.span,
    });
  }

  fn emit_expression_tokens(&mut self, start: usize, end: usize) {
    let span = Span::new(start as u32, end as u32);
    if let Some((_, tokens)) = self.parse_pure_expression(span)
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  fn emit_handler_tokens(&mut self, start: usize, end: usize) {
    let span = Span::new(start as u32, end as u32);
    let allocator = Allocator::new();
    let tokens = unsafe {
      self.parse_expression(span, b"(()=>{", b"})", &allocator, |expression| {
        let Expression::ArrowFunctionExpression(arrow) = expression else {
          return None;
        };

        let Some(first) = arrow.body.statements.first() else {
          return Some(((), Span::new(span.start, span.start)));
        };
        let last = arrow.body.statements.last().unwrap_or(first);

        Some(((), Span::new(first.span().start, last.span().end)))
      })
    };

    if let Some(((), tokens)) = tokens
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  fn emit_slot_params_tokens(&mut self, start: usize, end: usize) {
    let start = self.skip_ws(start, end);
    let end = self.trim_end_ws(start, end);
    if start >= end {
      return;
    }

    let span = Span::new(start as u32, end as u32);
    let is_parenthesized = self.byte(start) == b'(' && self.byte(end - 1) == b')';
    let (start_wrap, end_wrap): (&[u8], &[u8]) =
      if is_parenthesized { (b"(", b"=>0)") } else { (b"((", b")=>0)") };

    let allocator = Allocator::new();
    let tokens = unsafe {
      self.parse_expression(span, start_wrap, end_wrap, &allocator, |expression| {
        let Expression::ArrowFunctionExpression(arrow) = expression else {
          return None;
        };

        let token_span = if is_parenthesized {
          span
        } else if let Some(first) = arrow.params.items.first() {
          let end = arrow.params.rest.as_ref().map_or_else(
            || arrow.params.items.last().map_or(first.span.end, |last| last.span.end),
            |rest| rest.span.end,
          );
          Span::new(first.span.start, end)
        } else if let Some(rest) = &arrow.params.rest {
          rest.span
        } else {
          Span::new(span.start, span.start)
        };

        Some(((), token_span))
      })
    };

    if let Some(((), tokens)) = tokens
      && !tokens.is_empty()
    {
      self.push_template_oxc_tokens(tokens);
    }
  }

  fn emit_v_for_tokens(&mut self, start: usize, end: usize) {
    let Some(parts) = self.split_v_for_expression(start, end) else {
      self.emit_expression_tokens(start, end);
      return;
    };

    self.emit_slot_params_tokens(parts.left_start, parts.left_end);
    self.push_manual_oxc_token(
      if parts.operator == "in" { "Keyword" } else { "Identifier" },
      parts.operator,
      parts.operator_start,
      parts.operator_end,
    );
    self.emit_expression_tokens(parts.right_start, parts.right_end);
  }

  fn push_template_oxc_tokens(&mut self, tokens: &'a str) {
    if !tokens.is_empty() {
      self.sfc.template_tokens.push(tokens.into());
    }
  }

  fn push_manual_oxc_token(&mut self, token_type: &str, value: &str, start: usize, end: usize) {
    let token =
      format!(r#"{{"type":"{token_type}","value":"{value}","start":{start},"end":{end}}}"#);
    let token = self.vue_allocator.alloc_str(&token);
    self.sfc.template_tokens.push(token.into());
  }

  fn push_script_punctuator(&mut self, start: usize, end: usize, value: &'static str) {
    self.sfc.script_tokens.push(
      VToken::new(VTokenKind::Punctuator, Span::new(start as u32, end as u32), Some(value)).into(),
    );
  }

  fn push_template_vtoken(&mut self, token: VToken<'b>) {
    self.push_template_token(token.kind, token_start(token), token_end(token), token.value);
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

  fn push_template_token(
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

  fn split_v_for_expression(&self, start: usize, end: usize) -> Option<VForParts<'b>> {
    let mut pos = start;
    let mut paren_depth = 0_u32;
    let mut bracket_depth = 0_u32;
    let mut brace_depth = 0_u32;

    while pos < end {
      match self.byte(pos) {
        b'\'' | b'"' => {
          pos = self.skip_quoted(pos, end);
          continue;
        }
        b'`' => {
          pos = self.skip_template_literal(pos, end);
          continue;
        }
        b'(' => paren_depth += 1,
        b')' => paren_depth = paren_depth.saturating_sub(1),
        b'[' => bracket_depth += 1,
        b']' => bracket_depth = bracket_depth.saturating_sub(1),
        b'{' => brace_depth += 1,
        b'}' => brace_depth = brace_depth.saturating_sub(1),
        byte
          if byte.is_ascii_whitespace()
            && paren_depth == 0
            && bracket_depth == 0
            && brace_depth == 0 =>
        {
          let operator_start = self.skip_ws(pos, end);
          let operator_source = &self.source_text[operator_start..end];
          let operator_end =
            if operator_source.starts_with("in") || operator_source.starts_with("of") {
              operator_start + 2
            } else {
              pos += 1;
              continue;
            };

          if operator_end >= end || !self.byte(operator_end).is_ascii_whitespace() {
            pos += 1;
            continue;
          }

          let left_start = self.skip_ws(start, pos);
          let left_end = self.trim_end_ws(left_start, pos);
          let right_start = self.skip_ws(operator_end, end);
          let right_end = self.trim_end_ws(right_start, end);
          if left_start >= left_end || right_start >= right_end {
            return None;
          }

          return Some(VForParts {
            left_start,
            left_end,
            operator: &self.source_text[operator_start..operator_end],
            operator_start,
            operator_end,
            right_start,
            right_end,
          });
        }
        _ => {}
      }
      pos += 1;
    }

    None
  }

  fn alloc_str(&self, value: &str) -> &'b str {
    self.js_allocator.alloc_str(value)
  }

  fn skip_ws(&self, mut pos: usize, end: usize) -> usize {
    while pos < end && self.byte(pos).is_ascii_whitespace() {
      pos += 1;
    }
    pos
  }

  fn trim_end_ws(&self, start: usize, mut end: usize) -> usize {
    while end > start && self.byte(end - 1).is_ascii_whitespace() {
      end -= 1;
    }
    end
  }

  fn skip_quoted(&self, mut pos: usize, end: usize) -> usize {
    let quote = self.byte(pos);
    pos += 1;
    while pos < end {
      match self.byte(pos) {
        b'\\' => pos = (pos + 2).min(end),
        byte if byte == quote => return pos + 1,
        _ => pos += 1,
      }
    }
    pos
  }

  fn skip_template_literal(&self, mut pos: usize, end: usize) -> usize {
    pos += 1;
    while pos < end {
      match self.byte(pos) {
        b'\\' => pos = (pos + 2).min(end),
        b'`' => return pos + 1,
        _ => pos += 1,
      }
    }
    pos
  }

  fn byte(&self, pos: usize) -> u8 {
    self.source_text.as_bytes()[pos]
  }
}

fn update_lexer_mode(lexer: &mut Lexer<'_>, element_stack: &[ElementState]) {
  if element_stack.iter().rev().any(|element| element.v_pre) {
    lexer.set_mode(LexerMode::VPre);
  } else if is_foreign_content(element_stack) {
    lexer.set_mode(LexerMode::ForeignContent);
  } else {
    lexer.set_mode(LexerMode::Data);
  }
}

const fn token_start(token: VToken<'_>) -> usize {
  token.span.start as usize
}

const fn token_end(token: VToken<'_>) -> usize {
  token.span.end as usize
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

fn is_raw_text_element(name: &str) -> bool {
  matches!(
    name,
    "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript" | "plaintext"
  )
}

fn is_rcdata_element(name: &str) -> bool {
  matches!(name, "textarea" | "title")
}

fn is_foreign_content(element_stack: &[ElementState]) -> bool {
  element_stack.iter().rev().any(|element| matches!(element.name.as_str(), "svg" | "math"))
}

fn pop_element(element_stack: &mut Vec<ElementState>, name: &str) {
  if let Some(index) = element_stack.iter().rposition(|current| current.name == name) {
    element_stack.truncate(index);
  }
}
