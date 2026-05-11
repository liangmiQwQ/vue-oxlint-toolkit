use crate::VueParser;
use crate::lexer::{Lexer, LexerMode, VToken, VTokenKind};
use crate::parser::parse::state::{
  CurrentTag, ElementState, PendingScript, RawElement, ScriptInfo, TagAttrs,
};
use crate::parser::parse::{token_end, token_start};
use oxc_span::{SourceType, Span};

impl<'a, 'b> VueParser<'a, 'b>
where
  'b: 'a,
{
  pub(super) fn handle_tag_open(
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

  #[allow(clippy::too_many_arguments)]
  pub(super) fn finish_tag(
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
