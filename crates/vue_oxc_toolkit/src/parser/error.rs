use oxc_span::Span;

use oxc_diagnostics::OxcDiagnostic;
use vize_armature::{CompilerError, ErrorCode};

use crate::utils::VizeSpan;

/// Process vize parser errors into OXC diagnostics.
/// Returns the diagnostics and whether the parser should be considered panicked (fatal).
pub fn process_vize_errors(errors: &[CompilerError], diagnostics: &mut Vec<OxcDiagnostic>) -> bool {
  let mut panicked = false;
  for error in errors {
    if !is_warn(error.code) {
      if should_panic(error.code) {
        panicked = true;
      }
      let diag = OxcDiagnostic::error(error.to_string());
      diagnostics.push(if let Some(loc) = &error.loc { diag.with_label(loc.span()) } else { diag });
    }
  }
  panicked
}

#[must_use]
const fn is_warn(code: ErrorCode) -> bool {
  matches!(
    code,
    ErrorCode::InvalidFirstCharacterOfTagName
      | ErrorCode::NestedComment
      | ErrorCode::IncorrectlyClosedComment
      | ErrorCode::IncorrectlyOpenedComment
      | ErrorCode::AbruptClosingOfEmptyComment
      | ErrorCode::MissingWhitespaceBetweenAttributes
      | ErrorCode::MissingDirectiveName
  )
}

#[must_use]
const fn should_panic(code: ErrorCode) -> bool {
  matches!(
    code,
    // EOF errors - incomplete template structure
    ErrorCode::EofInTag
      | ErrorCode::EofInComment
      | ErrorCode::EofInCdata
      | ErrorCode::EofBeforeTagName
      | ErrorCode::EofInScriptHtmlCommentLikeText
      // Vue syntax incomplete - can't generate valid JSX
      | ErrorCode::MissingInterpolationEnd
      | ErrorCode::MissingDynamicDirectiveArgumentEnd
      | ErrorCode::MissingEndTag
      // Critical structural issues
      | ErrorCode::UnexpectedNullCharacter
      | ErrorCode::CdataInHtmlContent
  )
}

#[cold]
pub fn unexpected_script_lang(errors: &mut Vec<OxcDiagnostic>, lang: &str) {
  errors.push(OxcDiagnostic::error(format!("Unsupported lang {lang} in <script> blocks.")));
}

#[cold]
pub fn multiple_script_langs(errors: &mut Vec<OxcDiagnostic>) {
  errors
    .push(OxcDiagnostic::error("<script> and <script setup> must have the same language type."));
}

#[cold]
pub fn multiple_script_tags(errors: &mut Vec<OxcDiagnostic>, span: Span) {
  errors.push(
    OxcDiagnostic::error("Single file component can contain only one <script> element.")
      .with_label(span),
  );
}

#[cold]
pub fn multiple_script_setup_tags(errors: &mut Vec<OxcDiagnostic>, span: Span) {
  errors.push(
    OxcDiagnostic::error("Single file component can contain only one <script setup> element.")
      .with_label(span),
  );
}

#[cold]
pub fn v_else_without_adjacent_if(errors: &mut Vec<OxcDiagnostic>, span: Span) {
  errors.push(
    OxcDiagnostic::error("v-else/v-else-if has no adjacent v-if or v-else-if.").with_label(span),
  );
}

#[cold]
pub fn invalid_v_for_expression(errors: &mut Vec<OxcDiagnostic>, span: Span) {
  errors.push(OxcDiagnostic::error("v-for has invalid expression.").with_label(span));
}

#[cold]
pub fn v_if_else_without_expression(errors: &mut Vec<OxcDiagnostic>, span: Span) {
  errors.push(OxcDiagnostic::error("v-if/v-else-if is missing expression.").with_label(span));
}
