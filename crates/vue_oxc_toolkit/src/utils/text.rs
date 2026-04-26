use regex::Regex;

/// Convert kebab-case to camel-like case.
///
/// `pascal: true` -> `PascalCase` (e.g. `keep-alive` -> `KeepAlive`)
/// `pascal: false` -> `camelCase`  (e.g. `msg-id` -> `msgId`)
pub fn kebab_to_case(s: &str, pascal: bool) -> String {
  let mut result = String::with_capacity(s.len());
  let mut capitalize_next = pascal;
  for ch in s.chars() {
    if ch == '-' {
      capitalize_next = true;
    } else if capitalize_next {
      result.extend(ch.to_uppercase());
      capitalize_next = false;
    } else {
      result.push(ch);
    }
  }
  result
}

/// Walk back from `end` over trailing whitespace in `source` and return the
/// resulting offset. Used for vize fields whose `loc.end` may include trailing
/// whitespace before the next token.
pub fn roffset(source: &str, end: u32) -> u32 {
  let end = end as usize;
  let trimmed = end - source[..end].chars().rev().take_while(|c| c.is_whitespace()).count();
  trimmed as u32
}

/// Result of parsing a `v-for` alias expression.
///
/// All offsets are byte offsets within the original expression
/// (not within the SFC source).
pub struct ForAlias<'a> {
  /// The aliases part — `(item, index)` or `item`.
  pub aliases: &'a str,
  pub aliases_start: usize,
  pub aliases_end: usize,
  pub source_start: usize,
  pub source_end: usize,
}

/// Parse a `v-for` alias expression into its `(aliases, source)` components.
///
/// Mirrors Vue core's [`forAliasRE`].
///
/// [`forAliasRE`]: https://github.com/vuejs/core/blob/e1ccd9fde8f57fe7bd40fdf1345692ab3e6a1fa0/packages/compiler-core/src/utils.ts#L571
pub fn parse_v_for_alias(expression: &str) -> Option<ForAlias<'_>> {
  // The regex below is compiled on every call. v-for parsing only happens
  // for elements that actually use v-for, so we accept the cost here rather
  // than introducing a `static` cell.
  let re = Regex::new(r"^([\s\S]*?)\s+(?:in|of)\s+(\S[\s\S]*)").unwrap();
  let caps = re.captures(expression)?;
  let aliases = caps.get(1)?;
  let source = caps.get(2)?;
  Some(ForAlias {
    aliases: aliases.as_str(),
    aliases_start: aliases.start(),
    aliases_end: aliases.end(),
    source_start: source.start(),
    source_end: source.end(),
  })
}
