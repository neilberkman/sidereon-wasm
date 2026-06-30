//! Error mapping. Every fallible export returns `Result<_, JsValue>` so a
//! failure surfaces as a thrown JS exception rather than a wasm trap or panic.
//!
//! Engine failures become a plain `Error` carrying the engine's own message.
//! Bad caller input becomes `TypeError` (wrong shape / unparseable token) or
//! `RangeError` (out-of-domain / non-finite numbers), matching what a JS
//! developer expects from a native API.

use wasm_bindgen::prelude::*;

/// A domain failure from the engine: a parse rejection, a non-converging solve,
/// or an SGP4 error code. Carries the engine's message verbatim.
pub fn engine_error<E: core::fmt::Display>(err: E) -> JsValue {
    js_sys::Error::new(&err.to_string()).into()
}

/// Caller passed input of the wrong shape or an unparseable token.
pub fn type_error(message: &str) -> JsValue {
    js_sys::TypeError::new(message).into()
}

/// Caller passed an out-of-domain or non-finite numeric value.
pub fn range_error(message: &str) -> JsValue {
    js_sys::RangeError::new(message).into()
}

/// Decode a caller byte buffer as UTF-8 text, or a `TypeError`. Every RINEX
/// surface parses text, so the bytes the JS side hands in (a file read as a
/// `Uint8Array`) must be valid UTF-8.
pub fn utf8_text(bytes: &[u8], label: &str) -> Result<String, JsValue> {
    core::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|e| type_error(&format!("{label} is not valid UTF-8 text: {e}")))
}

/// Reject a non-finite scalar with a `RangeError` naming the field.
pub fn require_finite(value: f64, field: &str) -> Result<f64, JsValue> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(range_error(&format!("{field} must be a finite number")))
    }
}
