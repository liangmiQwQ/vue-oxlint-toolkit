use oxc_span::Span;

type DirectivePart<'s> = (&'s str, usize);

pub(super) fn is_directive_name(name: &str) -> bool {
  name.starts_with("v-") || name.starts_with(':') || name.starts_with('@') || name.starts_with('#')
}

pub(super) fn split_directive_argument(
  rest: &str,
) -> (Option<DirectivePart<'_>>, Vec<DirectivePart<'_>>) {
  let offset = usize::from(rest.starts_with(':'));
  let rest_without_colon = &rest[offset..];
  let mut parts = rest_without_colon.split('.');
  let argument = parts
    .next()
    .and_then(|argument| if argument.is_empty() { None } else { Some((argument, offset)) });

  let mut search_start = offset + argument.map_or(0, |(argument, _)| argument.len());
  let mut modifiers = Vec::new();
  for modifier in parts {
    search_start += 1;
    if !modifier.is_empty() {
      modifiers.push((modifier, search_start));
    }
    search_start += modifier.len();
  }

  (argument, modifiers)
}

pub(super) fn split_v_for_expression(source: &str) -> Option<(&str, &str, usize)> {
  for operator in [" in ", " of "] {
    if let Some(index) = source.find(operator) {
      return Some((&source[..index], &source[index + operator.len()..], index));
    }
  }

  None
}

pub(super) fn trimmed_sub_span(parent: Span, child: &str, parent_source: &str) -> Span {
  let leading = child.len() - child.trim_start().len();
  let len = child.trim().len();
  let start = parent_source.find(child).unwrap_or_default() + leading;
  Span::new(parent.start + start as u32, parent.start + start as u32 + len as u32)
}

pub(super) fn is_raw_text_tag(name: &str) -> bool {
  matches!(
    name.to_ascii_lowercase().as_str(),
    "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript" | "plaintext"
  )
}

pub(super) fn is_rc_data_tag(name: &str) -> bool {
  matches!(name.to_ascii_lowercase().as_str(), "textarea" | "title")
}

pub(super) fn is_void_tag(name: &str) -> bool {
  matches!(
    name.to_ascii_lowercase().as_str(),
    "area"
      | "base"
      | "br"
      | "col"
      | "embed"
      | "hr"
      | "img"
      | "input"
      | "link"
      | "meta"
      | "param"
      | "source"
      | "track"
      | "wbr"
  )
}
