use oxc_ast::Comment;
use oxc_ast::ast::CommentKind;
use oxc_span::Span;

use crate::source_text::SourceOffsets;

use super::types::NativeComment;

pub fn native_comment(
  source_text: &str,
  offsets: &SourceOffsets,
  comment: &Comment,
) -> NativeComment {
  let comment_data = comment_data(source_text, comment.kind, comment.span.start, comment.span.end);
  let span = Span::new(comment_data.start, comment_data.end);

  NativeComment {
    r#type: match comment.kind {
      CommentKind::Line => "Line",
      CommentKind::SingleLineBlock | CommentKind::MultiLineBlock => "Block",
    }
    .to_string(),
    value: comment_data.value.to_string(),
    start: offsets.offset(comment_data.start),
    end: offsets.offset(comment_data.end),
    range: offsets.range(span),
  }
}

struct CommentData<'a> {
  value: &'a str,
  start: u32,
  end: u32,
}

fn comment_data(source_text: &str, kind: CommentKind, start: u32, end: u32) -> CommentData<'_> {
  let start = start as usize;
  let end = end as usize;

  if kind == CommentKind::Line {
    let value_start = start + 2;
    let end = line_comment_end(source_text, value_start);

    return CommentData {
      value: source_text.get(value_start..end).unwrap_or_default(),
      start: start as u32,
      end: end as u32,
    };
  }

  CommentData {
    value: source_text.get(start..end).unwrap_or_default(),
    start: start as u32,
    end: end as u32,
  }
}

fn line_comment_end(source_text: &str, value_start: usize) -> usize {
  source_text[value_start..].find('\n').map_or(source_text.len(), |newline| value_start + newline)
}
