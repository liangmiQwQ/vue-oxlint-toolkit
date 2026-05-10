use napi_derive::napi;

#[napi(object)]
pub struct NativeComment {
  #[napi(ts_type = "'Line' | 'Block'")]
  pub r#type: String,
  pub value: String,
  pub start: u32,
  pub end: u32,
  #[napi(ts_type = "[number, number]")]
  pub range: Vec<u32>,
}

#[napi(object)]
pub struct NativeDiagnostic {
  pub message: String,
  pub start: u32,
  pub end: u32,
}

#[napi(object)]
pub struct NativeMapping {
  pub virtual_start: u32,
  pub virtual_end: u32,
  pub original_start: u32,
  pub original_end: u32,
}

#[napi(object)]
pub struct NativeTransformResult {
  pub source_text: String,
  #[napi(ts_type = "'jsx' | 'tsx'")]
  pub script_kind: String,
  pub comments: Vec<NativeComment>,
  #[napi(ts_type = "Array<[number, number]>")]
  pub irregular_whitespaces: Vec<Vec<u32>>,
  pub errors: Vec<NativeDiagnostic>,
  pub mappings: Vec<NativeMapping>,
}
