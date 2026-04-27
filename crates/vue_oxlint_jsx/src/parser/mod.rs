use std::ptr;

use oxc_allocator::{Allocator, CloneIn, Vec as ArenaVec};
use oxc_ast::{
  AstBuilder, Comment,
  ast::{Directive, Program, Statement},
};
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::ParseOptions;
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::ModuleRecord;

mod elements;
mod error;
mod modules;
mod parse;
mod script;

pub struct ScriptBlock<'a> {
  directives: ArenaVec<'a, Directive<'a>>,
  statements: ArenaVec<'a, Statement<'a>>,
}

pub struct ParserImpl<'a> {
  allocator: &'a Allocator,
  origin_source_text: &'a str,
  options: ParseOptions,

  comments: ArenaVec<'a, Comment>,
  source_type: SourceType,
  module_record: ModuleRecord<'a>,
  errors: Vec<OxcDiagnostic>,

  source_text: &'a str,
  mut_ptr_source_text: *mut [u8],
  ast: AstBuilder<'a>,
  script_set: bool,
  setup_set: bool,

  global: ScriptBlock<'a>,
  setup: ScriptBlock<'a>,
  sfc_struct_jsx_statement: Option<Statement<'a>>,
}

impl<'a> ParserImpl<'a> {
  /// Create a [`ParserImpl`]
  pub fn new(allocator: &'a Allocator, source_text: &'a str, options: ParseOptions) -> Self {
    let ast = AstBuilder::new(allocator);
    let alloced_str = allocator.alloc_slice_copy(source_text.as_bytes());

    Self {
      allocator,
      origin_source_text: source_text,
      options,

      comments: ast.vec(),
      source_type: SourceType::mjs().with_unambiguous(true),
      module_record: ModuleRecord::new(allocator),
      errors: vec![],

      mut_ptr_source_text: ptr::from_mut(alloced_str),
      // SAFETY: alloced_str is from a `&str`
      source_text: unsafe { str::from_utf8_unchecked(alloced_str) },
      ast,
      script_set: false,
      setup_set: false,

      global: ScriptBlock { directives: ast.vec(), statements: ast.vec() },
      setup: ScriptBlock { directives: ast.vec(), statements: ast.vec() },
      sfc_struct_jsx_statement: None,
    }
  }
}

pub struct ParserImplReturn<'a> {
  pub program: Program<'a>,
  pub module_record: ModuleRecord<'a>,

  pub fatal: bool,
  pub errors: Vec<OxcDiagnostic>,
}

// Some public utils
impl<'a, 'b> ParserImpl<'a>
where
  'a: 'b,
{
  pub const fn sync_source_text(&mut self) {
    // SAFETY: `self.origin_source_text` has the same length as `self.mut_ptr_source_text`, the former's data is in heap, while the latter's data is in arena, so no overlapping
    unsafe {
      ptr::copy_nonoverlapping(
        self.origin_source_text.as_ptr(),
        self.mut_ptr_source_text.cast(),
        self.origin_source_text.len(),
      );
    }
  }

  /// Call [`oxc_parser::Parser::parse`] with a custom wrap
  /// Everything before `start` and `start_wrap` will be ignored
  ///
  /// If you need to parse with any wrapper, it will produce unused AST nodes
  /// `allocator` param should provided and drop unused AST nodes
  pub fn oxc_parse(
    &mut self,
    span: Span,
    start_wrap: &[u8],
    end_wrap: &[u8],
    allocator: Option<&'b Allocator>,
  ) -> Option<(ArenaVec<'b, Directive<'b>>, ArenaVec<'b, Statement<'b>>, ModuleRecord<'b>)> {
    let start = span.start as usize;
    let end = span.end as usize;

    // SAFETY: we don't edit between `start` and `end`, and reset before returning
    unsafe {
      let real_start = start - start_wrap.len();
      let first_byte_ptr = self.mut_ptr_source_text.cast::<u8>();

      // Copy start_wrap to the front of the source text
      ptr::copy_nonoverlapping(
        start_wrap.as_ptr(),
        first_byte_ptr.add(real_start),
        start_wrap.len(),
      );
      // Copy end_wrap to the end of the source text
      ptr::copy_nonoverlapping(end_wrap.as_ptr(), first_byte_ptr.add(end), end_wrap.len());

      // Pad source with space
      for i in 0..real_start {
        first_byte_ptr.add(i).write(b' ');
      }
    }

    // SAFETY: it must be a valid utf-8 string
    let result = self.call_oxc_parse(
      unsafe { str::from_utf8_unchecked(&self.source_text.as_bytes()[..end + end_wrap.len()]) },
      allocator.unwrap_or(self.allocator),
    );

    // Reset
    self.sync_source_text();
    result
  }

  fn call_oxc_parse(
    &mut self,
    source: &'a str,
    allocator: &'b Allocator,
  ) -> Option<(ArenaVec<'b, Directive<'b>>, ArenaVec<'b, Statement<'b>>, ModuleRecord<'b>)> {
    let mut ret = oxc_parser::Parser::new(allocator, source, self.source_type)
      .with_options(self.options)
      .parse();

    self.errors.append(&mut ret.errors);
    if ret.panicked {
      None
    } else {
      let mut comments = ret.program.comments.clone_in(self.allocator);
      self.comments.append(&mut comments);
      Some((ret.program.directives, ret.program.body, ret.module_record))
    }
  }
}

#[macro_export]
macro_rules! is_void_tag {
  ($name:ident) => {
    matches!(
      $name,
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
  };
}

/// For inner parser implement use. Use Result<T, ()> for fn which may make parser panic
type ResParse<T> = Result<T, ()>;

trait ResParseExt<T> {
  fn panic() -> ResParse<T> {
    Err(())
  }

  // do not use `ok` as name, because it is a method of Result
  fn success(t: T) -> ResParse<T> {
    Ok(t)
  }
}

impl<T> ResParseExt<T> for ResParse<T> {}
