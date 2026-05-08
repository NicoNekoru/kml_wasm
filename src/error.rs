use std::fmt;
use wasm_bindgen::JsValue;

#[derive(Debug)]
pub struct CompileError {
    pub message: String,
    pub offset: Option<usize>,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(o) = self.offset {
            write!(f, "{} at offset {}", self.message, o)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for CompileError {}

impl From<CompileError> for JsValue {
    fn from(e: CompileError) -> JsValue {
        JsValue::from_str(&e.to_string())
    }
}

pub fn err<T>(msg: impl Into<String>, offset: Option<usize>) -> Result<T, CompileError> {
    Err(CompileError {
        message: msg.into(),
        offset,
    })
}
