use oxc_allocator::{Box as ArenaBox, Vec as ArenaVec};
use oxc_span::{GetSpan, Span};

use crate::ast::{VComment, VEndTag, VInterpolation, VNode, VPureScript, VText};
use crate::lexer::{VToken, VTokenKind};
use crate::parser::module_record::merge_module_record;
use crate::{error, parser::parse::TemplateParser};

impl<'a, 'b> TemplateParser<'_, 'a, 'b>
where
  'b: 'a,
{
  pub(super) fn parse_children(&mut self, until: Option<&str>) -> ArenaVec<'a, VNode<'a, 'b>> {
    let mut children = ArenaVec::new_in(self.parser.vue_allocator);

    while let Some(token) = self.peek() {
      match token.kind {
        VTokenKind::HTMLEndTagOpen => {
          if until.is_some_and(|name| self.next_end_tag_matches(name)) {
            break;
          }
          self.consume_unmatched_end_tag();
        }
        VTokenKind::HTMLTagOpen => {
          if let Some(node) = self.parse_element() {
            children.push(node);
          }
        }
        VTokenKind::VExpressionStart => {
          if let Some(node) = self.parse_interpolation() {
            children.push(node);
          }
        }
        VTokenKind::HTMLComment | VTokenKind::HTMLBogusComment => {
          // SAFETY: `peek()` proved the token exists and `next()` consumes that same token.
          let token = self.next().unwrap();
          let value = self.alloc_value(token.value.unwrap_or_default());
          self.parser.sfc.template_comments.push(VComment {
            r#type: token.kind.comment_type(),
            value,
            span: token.span,
          });
        }
        VTokenKind::HTMLText
        | VTokenKind::HTMLWhitespace
        | VTokenKind::HTMLRawText
        | VTokenKind::HTMLRCDataText
        | VTokenKind::HTMLCDataText => {
          // SAFETY: `peek()` proved the token exists and `next()` consumes that same token.
          let token = self.next().unwrap();
          if let Some(node) = self.text_node(token) {
            children.push(node);
          }
        }
        _ => {
          // SAFETY: `peek()` proved the token exists and `next()` consumes that same token.
          let token = self.next().unwrap();
          self.parser.sfc.template_tokens.push(token.into());
        }
      }
    }

    children
  }

  pub(super) fn parse_raw_text_children(&mut self, is_script: bool) -> ArenaVec<'a, VNode<'a, 'b>> {
    let mut children = ArenaVec::new_in(self.parser.vue_allocator);
    let mut raw_start = None;
    let mut raw_end = None;

    while let Some(token) = self.peek() {
      if token.kind == VTokenKind::HTMLEndTagOpen {
        break;
      }

      // SAFETY: `peek()` proved the token exists and `next()` consumes that same token.
      let token = self.next().unwrap();
      self.parser.sfc.template_tokens.push(token.into());
      raw_start.get_or_insert(token.span.start);
      raw_end = Some(token.span.end);
    }

    let Some(start) = raw_start else {
      return children;
    };
    let span = Span::new(start, raw_end.unwrap_or(start));

    if is_script && let Some(script) = self.parse_script(span) {
      children.push(script);
      return children;
    }

    let text = self.alloc_value(span.source_text(self.parser.source_text));
    children.push(VNode::Text(ArenaBox::new_in(VText { text, span }, self.parser.vue_allocator)));
    children
  }

  fn parse_script(&mut self, span: Span) -> Option<VNode<'a, 'b>> {
    if span.source_text(self.parser.source_text).trim().is_empty() {
      return None;
    }

    let ret = self.parser.oxc_parse(span, &[], &[], Some(self.parser.js_allocator))?;

    for directive in &ret.directives {
      self.parser.clean_spans.insert(directive.span());
    }
    for statement in &ret.statements {
      self.parser.clean_spans.insert(statement.span());
    }

    merge_module_record(&mut self.parser.module_record, ret.module_record);
    if !ret.tokens.is_empty() {
      self.parser.sfc.script_tokens.push(ret.tokens.into());
    }

    let script = VPureScript { statements: ret.statements, directives: ret.directives, span };
    Some(VNode::PureScript(ArenaBox::new_in(script, self.parser.vue_allocator)))
  }

  fn parse_interpolation(&mut self) -> Option<VNode<'a, 'b>> {
    let start = self.next()?;
    self.parser.sfc.template_tokens.push(start.into());
    let expression = self.next()?;
    self.parser.sfc.template_tokens.push(expression.into());
    let end = self.next()?;
    self.parser.sfc.template_tokens.push(end.into());

    if expression.kind != VTokenKind::HTMLText || end.kind != VTokenKind::VExpressionEnd {
      self.parser.errors.push(error::unexpected_token(expression.span, "interpolation expression"));
      return None;
    }

    let span = Span::new(expression.span.start, expression.span.end);
    let Some((expression, references, tokens)) = self.parser.parse_pure_expression(span) else {
      return self.text_node(VToken::new(
        VTokenKind::HTMLText,
        Span::new(start.span.start, end.span.end),
        Some(&self.parser.source_text[start.span.start as usize..end.span.end as usize]),
      ));
    };

    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }
    let interpolation =
      VInterpolation { expression, references, span: Span::new(start.span.start, end.span.end) };
    Some(VNode::Interpolation(ArenaBox::new_in(interpolation, self.parser.vue_allocator)))
  }

  pub(super) fn consume_end_tag(&mut self, name: &str) -> Option<VEndTag> {
    if !self.peek().is_some_and(|token| token.kind == VTokenKind::HTMLEndTagOpen) {
      return None;
    }

    // SAFETY: the guard above proves the next token is an end-tag opener.
    let open = self.next().unwrap();
    self.parser.sfc.template_tokens.push(open.into());
    let name_token = self.next_non_ws()?;
    self.parser.sfc.template_tokens.push(name_token.into());
    if !name_token.value.unwrap_or_default().eq_ignore_ascii_case(name) {
      self.parser.errors.push(error::unexpected_closing_tag(name_token.span));
    }

    let mut end = name_token.span.end;
    while let Some(token) = self.next() {
      end = token.span.end;
      let should_break = token.kind == VTokenKind::HTMLTagClose;
      self.parser.sfc.template_tokens.push(token.into());
      if should_break {
        break;
      }
    }

    Some(VEndTag { span: Span::new(open.span.start, end) })
  }

  fn consume_unmatched_end_tag(&mut self) {
    // SAFETY: callers only enter this path after seeing `HTMLEndTagOpen`.
    let start = self.next().unwrap();
    self.parser.sfc.template_tokens.push(start.into());
    let mut unexpected_span = start.span;
    while let Some(token) = self.next() {
      let should_break = token.kind == VTokenKind::HTMLTagClose;
      if token.kind == VTokenKind::HTMLIdentifier {
        unexpected_span = Span::new(start.span.start, token.span.end);
      }
      self.parser.sfc.template_tokens.push(token.into());
      if should_break {
        break;
      }
    }
    self.parser.errors.push(error::unexpected_closing_tag(unexpected_span));
  }

  fn next_end_tag_matches(&self, name: &str) -> bool {
    let mut peeked = self.peeked;
    let mut lexer = self.lexer.clone();
    let mut next = || peeked.take().or_else(|| lexer.next_token());

    if !next().is_some_and(|token| token.kind == VTokenKind::HTMLEndTagOpen) {
      return false;
    }

    while let Some(token) = next() {
      if token.kind == VTokenKind::HTMLWhitespace {
        continue;
      }

      return token.kind == VTokenKind::HTMLIdentifier
        && token.value.unwrap_or_default().eq_ignore_ascii_case(name);
    }

    false
  }

  fn text_node(&mut self, token: VToken<'b>) -> Option<VNode<'a, 'b>> {
    self.parser.sfc.template_tokens.push(token.into());
    let text = token.value?;
    let text = self.alloc_value(text);
    Some(VNode::Text(ArenaBox::new_in(VText { text, span: token.span }, self.parser.vue_allocator)))
  }
}

trait CommentToken {
  fn comment_type(self) -> &'static str;
}

impl CommentToken for VTokenKind {
  fn comment_type(self) -> &'static str {
    match self {
      Self::HTMLBogusComment => "HTMLBogusComment",
      _ => "HTMLComment",
    }
  }
}
