//! Attribute and directive parsing.

use oxc_span::Span;

use crate::ast::{
  DirectiveArgument, DirectiveName, VAttrOrDirective, VAttribute, VAttributeValue, VDirective,
};
use crate::parser::Parser;

impl<'a> Parser<'a> {
  /// Parse one attribute or directive from the current position.
  /// Returns None if we can't parse (likely at `>` or `/>` or EOF).
  pub fn parse_attribute(&mut self) -> Option<VAttrOrDirective<'a>> {
    let attr_start = self.pos_u32();

    // Read the attribute name
    let name = self.read_attr_name()?;
    if name.is_empty() {
      return None;
    }

    let name_end = self.pos_u32();

    self.skip_whitespace();

    // Check for '=' (value part)
    let (value_raw, value_span) = if self.current_byte() == Some(b'=') {
      self.advance(1); // skip '='
      self.skip_whitespace();
      self.read_attr_value()
    } else {
      (None, None)
    };

    let attr_end = self.pos_u32();
    let span = Span::new(attr_start, attr_end);
    let name_span = Span::new(attr_start, name_end);

    // Detect directives
    if is_directive_name(&name) {
      let directive = self.parse_directive(&name, name_span, value_raw, value_span, span);
      Some(VAttrOrDirective::Directive(directive))
    } else {
      let value = value_raw.map(|raw| VAttributeValue { span: value_span.unwrap_or(span), raw });
      Some(VAttrOrDirective::Attribute(VAttribute { name, name_span, value, span }))
    }
  }

  /// Read the attribute name (ASCII identifier-ish chars, plus `-`, `.`, `[`, `]`, `:`... for directives)
  fn read_attr_name(&mut self) -> Option<String> {
    let start = self.pos;
    while let Some(b) = self.current_byte() {
      match b {
        b'=' | b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r' => break,
        _ => self.pos += 1,
      }
    }
    if self.pos == start { None } else { Some(self.source_text[start..self.pos].to_string()) }
  }

  /// Read the attribute value: `"..."`, `'...'`, or unquoted token
  fn read_attr_value(&mut self) -> (Option<String>, Option<Span>) {
    match self.current_byte() {
      Some(b'"') => {
        self.advance(1);
        let start = self.pos_u32();
        while !self.is_eof() && self.current_byte() != Some(b'"') {
          self.pos += 1;
        }
        let end = self.pos_u32();
        if self.current_byte() == Some(b'"') {
          self.advance(1);
        }
        let raw = self.source_text[start as usize..end as usize].to_string();
        (Some(raw), Some(Span::new(start, end)))
      }
      Some(b'\'') => {
        self.advance(1);
        let start = self.pos_u32();
        while !self.is_eof() && self.current_byte() != Some(b'\'') {
          self.pos += 1;
        }
        let end = self.pos_u32();
        if self.current_byte() == Some(b'\'') {
          self.advance(1);
        }
        let raw = self.source_text[start as usize..end as usize].to_string();
        (Some(raw), Some(Span::new(start, end)))
      }
      _ => {
        // Unquoted
        let start = self.pos_u32();
        while let Some(b) = self.current_byte() {
          if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'>' || b == b'/' {
            break;
          }
          self.pos += 1;
        }
        let end = self.pos_u32();
        if start == end {
          (None, None)
        } else {
          let raw = self.source_text[start as usize..end as usize].to_string();
          (Some(raw), Some(Span::new(start, end)))
        }
      }
    }
  }

  /// Parse a directive from a name like `v-bind:class.mod1.mod2` or `:class` or `@click` or `#slot`
  pub fn parse_directive(
    &mut self,
    full_name: &str,
    name_span: Span,
    value_raw: Option<String>,
    value_span: Option<Span>,
    span: Span,
  ) -> VDirective<'a> {
    // Detect shorthand and normalize
    let (directive_name, arg_str, modifiers) = parse_directive_parts(full_name);

    let argument = arg_str.map(|(arg, is_dynamic)| {
      // The arg span is approximate (part of name_span)
      // We can compute a more exact span by finding ':' in the name
      let colon_pos = full_name.find(':').or_else(|| {
        // For shorthands ':' is at index 1
        if full_name.starts_with(':') || full_name.starts_with('@') || full_name.starts_with('#') {
          Some(0)
        } else {
          None
        }
      });

      let arg_start = colon_pos.map_or(name_span.end, |p| {
        // skip the colon and prefix
        let skip =
          if full_name.starts_with(':') || full_name.starts_with('@') || full_name.starts_with('#')
          {
            1
          } else {
            p + 1
          };
        name_span.start + skip as u32
      });

      // Compute end: stop at first '.' (modifier separator)
      let dot_pos = arg.find('.');
      let arg_end = dot_pos.map_or(name_span.end, |p| arg_start + p as u32);

      let arg_span = Span::new(arg_start, arg_end);

      // Remove modifiers from arg
      let clean_arg = dot_pos.map_or(arg.as_str(), |p| &arg[..p]).to_string();

      if is_dynamic {
        DirectiveArgument::Dynamic(clean_arg, arg_span)
      } else {
        DirectiveArgument::Static(clean_arg, arg_span)
      }
    });

    // Parse expression based on directive type
    let expression = if let (Some(vs), Some(vr)) = (value_span, value_raw.as_deref()) {
      self.parse_directive_expression(&directive_name, vs, Some(vr))
    } else {
      None
    };

    VDirective {
      name: directive_name,
      argument,
      modifiers,
      value_raw,
      value_span,
      expression,
      span,
    }
  }
}

/// Returns true if an attribute name is a directive
#[must_use]
pub fn is_directive_name(name: &str) -> bool {
  name.starts_with("v-") || name.starts_with(':') || name.starts_with('@') || name.starts_with('#')
}

/// Parse directive parts from full name.
/// Returns (`directive_name`, `Option<(arg_string, is_dynamic)>`, modifiers)
#[allow(clippy::option_if_let_else)]
fn parse_directive_parts(full_name: &str) -> (DirectiveName, Option<(String, bool)>, Vec<String>) {
  // Handle shorthands
  let (name_part, rest) = if let Some(rest) = full_name.strip_prefix("v-") {
    // v-name[:arg.mod1.mod2]
    let colon_pos = rest.find(':');
    let name_end = colon_pos.unwrap_or(rest.len());
    let name_part = &rest[..name_end];
    let arg_part = colon_pos.map(|p| &rest[p + 1..]);
    (name_part.to_string(), arg_part.map(String::from))
  } else if let Some(rest) = full_name.strip_prefix(':') {
    // :arg.mod1 → v-bind:arg.mod1
    ("bind".to_string(), Some(rest.to_string()))
  } else if let Some(rest) = full_name.strip_prefix('@') {
    // @arg.mod1 → v-on:arg.mod1
    ("on".to_string(), Some(rest.to_string()))
  } else if let Some(rest) = full_name.strip_prefix('#') {
    // #arg → v-slot:arg
    ("slot".to_string(), Some(rest.to_string()))
  } else {
    (full_name.to_string(), None)
  };

  // Parse arg and modifiers from rest
  let (arg, modifiers) = if let Some(rest) = rest {
    let parts: Vec<&str> = rest.splitn(2, '.').collect();
    let arg_raw = parts[0].to_string();

    let (arg_clean, is_dynamic) = if arg_raw.starts_with('[') && arg_raw.ends_with(']') {
      let inner = arg_raw[1..arg_raw.len() - 1].to_string();
      (inner, true)
    } else {
      (arg_raw, false)
    };

    let mods: Vec<String> = if parts.len() > 1 {
      parts[1].split('.').map(String::from).filter(|s| !s.is_empty()).collect()
    } else {
      Vec::new()
    };

    (Some((arg_clean, is_dynamic)), mods)
  } else {
    (None, Vec::new())
  };

  let directive_name = match name_part.as_str() {
    "for" => DirectiveName::For,
    "if" => DirectiveName::If,
    "else-if" => DirectiveName::ElseIf,
    "else" => DirectiveName::Else,
    "show" => DirectiveName::Show,
    "model" => DirectiveName::Model,
    "on" => DirectiveName::On,
    "bind" => DirectiveName::Bind,
    "slot" => DirectiveName::Slot,
    other => DirectiveName::Custom(other.to_string()),
  };

  (directive_name, arg, modifiers)
}
