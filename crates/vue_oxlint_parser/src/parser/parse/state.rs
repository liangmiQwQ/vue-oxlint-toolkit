use crate::lexer::VToken;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct TagAttrs<'s> {
  pub(super) setup: bool,
  pub(super) v_pre: bool,
  pub(super) lang: Option<&'s str>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingAttr<'s> {
  pub(super) name_start: usize,
  pub(super) name_end: usize,
  pub(super) name: &'s str,
  pub(super) association: Option<VToken<'s>>,
  pub(super) value: Option<VToken<'s>>,
}

#[derive(Debug)]
pub(super) struct CurrentTag<'s> {
  pub(super) name: &'s str,
  pub(super) normalized_name: String,
  pub(super) open_start: usize,
  pub(super) is_end: bool,
  pub(super) attrs: TagAttrs<'s>,
  pub(super) last_attr_name: Option<&'s str>,
  pub(super) attr_name_start: Option<usize>,
  pub(super) attr_name_end: usize,
  pub(super) pending_attrs: Vec<PendingAttr<'s>>,
  pub(super) awaiting_attr_value: Option<PendingAttr<'s>>,
}

#[derive(Debug, Clone)]
pub(super) struct ElementState {
  pub(super) name: String,
  pub(super) v_pre: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScriptInfo<'s> {
  pub(super) open_start: usize,
  pub(super) open_end: usize,
  pub(super) attrs: TagAttrs<'s>,
}

#[derive(Debug, Clone)]
pub(super) struct RawElement<'s> {
  pub(super) body_start: usize,
  pub(super) script: Option<ScriptInfo<'s>>,
}

#[derive(Debug, Clone)]
pub(super) struct PendingScript<'s> {
  pub(super) info: ScriptInfo<'s>,
  pub(super) body_start: usize,
  pub(super) body_end: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AttrValueKind {
  Literal,
  Expression,
  Handler,
  SlotParams,
  VFor,
}
