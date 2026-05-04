use oxc_ast::{Comment, CommentKind};
use oxc_estree::{ESTree, Serializer, StructSerializer};
use oxc_span::Span;

use crate::ast::VComment;

pub struct ESTreeComment<'a> {
  pub r#type: &'a str,
  pub value: &'a str,
  pub span: Span,
}

impl ESTree for ESTreeComment<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", self.r#type);
    state.serialize_field("value", self.value);
    state.serialize_span(self.span);
    state.end();
  }
}

impl<'a> ESTreeComment<'a> {
  pub fn from_oxc_comment(comment: &Comment, source_text: &'a str) -> Self {
    let span = comment.span;
    let (r#type, span) = match comment.kind {
      CommentKind::Line => ("Line", span.shrink_left(2)),
      CommentKind::SingleLineBlock | CommentKind::MultiLineBlock => ("Block", span.shrink(2)),
    };
    let value = span.source_text(source_text);

    Self { r#type, value, span }
  }

  pub fn from_v_comment(comment: &VComment<'a>) -> Self {
    Self { r#type: comment.r#type, value: comment.value, span: comment.span }
  }
}
