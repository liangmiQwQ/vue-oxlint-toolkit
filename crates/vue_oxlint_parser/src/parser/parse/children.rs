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
          let value = self.alloc_value(token.value);
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
          self.push_text_child(&mut children, token);
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

    let text = self.alloc_value(span.source_text(self.parser.source_text));
    children.push(VNode::Text(ArenaBox::new_in(VText { text, span }, self.parser.vue_allocator)));
    if is_script {
      self.parse_script(span);
    }
    children
  }

  fn parse_script(&mut self, span: Span) {
    if span.source_text(self.parser.source_text).trim().is_empty() {
      return;
    }

    let Some(ret) = self.parser.oxc_parse(span, &[], &[], Some(self.parser.js_allocator)) else {
      return;
    };

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
    self.parser.sfc.script_body.push(script);
    self.parser.sfc.script_span =
      Some(self.parser.sfc.script_span.map_or(span, |current| {
        Span::new(current.start.min(span.start), current.end.max(span.end))
      }));
  }

  fn parse_interpolation(&mut self) -> Option<VNode<'a, 'b>> {
    let start = self.next()?;
    self.parser.sfc.template_tokens.push(start.into());
    let expression = self.next()?;
    let end = self.next()?;

    if expression.kind != VTokenKind::HTMLText || end.kind != VTokenKind::VExpressionEnd {
      self.parser.errors.push(error::unexpected_token(expression.span, "interpolation expression"));
      return None;
    }

    let span = Span::new(expression.span.start, expression.span.end);
    let Some((expression, references, tokens)) = self.parser.parse_pure_expression(span) else {
      return Some(self.text_node(VToken::new(
        VTokenKind::HTMLText,
        Span::new(start.span.start, end.span.end),
        &self.parser.source_text[start.span.start as usize..end.span.end as usize],
      )));
    };

    if !tokens.is_empty() {
      self.parser.sfc.template_tokens.push(tokens.into());
    }
    self.parser.sfc.template_tokens.push(end.into());
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
    if !open.value.eq_ignore_ascii_case(name) {
      self.parser.errors.push(error::unexpected_closing_tag(open.span));
    }

    let mut end = open.span.end;
    while let Some(token) = self.next() {
      end = token.span.end;
      let should_break = token.kind == VTokenKind::HTMLTagClose;
      if token.kind != VTokenKind::HTMLWhitespace {
        self.parser.sfc.template_tokens.push(token.into());
      }
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
      if token.kind == VTokenKind::HTMLEndTagOpen {
        unexpected_span = Span::new(start.span.start, token.span.end);
      }
      if token.kind != VTokenKind::HTMLWhitespace {
        self.parser.sfc.template_tokens.push(token.into());
      }
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

    next().is_some_and(|token| {
      token.kind == VTokenKind::HTMLEndTagOpen && token.value.eq_ignore_ascii_case(name)
    })
  }

  fn text_node(&mut self, token: VToken<'b>) -> VNode<'a, 'b> {
    self.parser.sfc.template_tokens.push(token.into());
    let text = token.value;
    let text = self.alloc_value(text);
    VNode::Text(ArenaBox::new_in(VText { text, span: token.span }, self.parser.vue_allocator))
  }

  fn push_text_child(&mut self, children: &mut ArenaVec<'a, VNode<'a, 'b>>, token: VToken<'b>) {
    self.parser.sfc.template_tokens.push(token.into());
    if let Some(VNode::Text(last)) = children.last_mut()
      && last.span.end == token.span.start
    {
      let span = Span::new(last.span.start, token.span.end);
      last.text = self.alloc_value(span.source_text(self.parser.source_text));
      last.span = span;
      return;
    }

    let text = self.alloc_value(token.value);
    children.push(VNode::Text(ArenaBox::new_in(
      VText { text, span: token.span },
      self.parser.vue_allocator,
    )));
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
