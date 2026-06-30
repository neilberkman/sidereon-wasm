//! Compact RINEX (Hatanaka) observation-file decoding and encoding. Mirrors the
//! core `rinex::crinex` surface: decode CRINEX text back to plain RINEX OBS text,
//! and encode plain RINEX OBS text into a CRINEX stream.

use wasm_bindgen::prelude::*;

use sidereon_core::rinex::crinex::{
    decode as core_decode, decode_to as core_decode_to, encode_crinex as core_encode_crinex,
};

use crate::error::{engine_error, utf8_text};

/// Decode Compact RINEX (Hatanaka) bytes into plain RINEX OBS text. Throws a
/// `TypeError` on non-UTF-8 input and an `Error` on a decode failure.
#[wasm_bindgen(js_name = decodeCrinex)]
pub fn decode_crinex(bytes: &[u8]) -> Result<String, JsValue> {
    let text = utf8_text(bytes, "CRINEX source")?;
    core_decode(&text).map_err(engine_error)
}

/// Decode Compact RINEX bytes into plain RINEX OBS lines (each without its
/// trailing newline). Throws a `TypeError` on non-UTF-8 input and an `Error` on
/// a decode failure.
#[wasm_bindgen(js_name = decodeCrinexLines)]
pub fn decode_crinex_lines(bytes: &[u8]) -> Result<Vec<String>, JsValue> {
    let text = utf8_text(bytes, "CRINEX source")?;
    let mut lines = Vec::new();
    core_decode_to(&text, |line| lines.push(line.to_owned())).map_err(engine_error)?;
    Ok(lines)
}

/// Alias of [`decodeCrinex`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadCrinex)]
pub fn load_crinex(bytes: &[u8]) -> Result<String, JsValue> {
    decode_crinex(bytes)
}

/// Encode plain RINEX observation text into a Compact RINEX (Hatanaka) stream,
/// the inverse of [`decodeCrinex`]. RINEX 2 is emitted as CRINEX 1.0 and RINEX 3
/// as CRINEX 3.0, selected from the embedded `RINEX VERSION / TYPE` header. The
/// output is the canonical all-reset compression form, so it round-trips
/// (`decodeCrinex(encodeCrinex(rinex)) == rinex`) but is not byte-identical to an
/// arbitrary `RNX2CRX` stream. Throws an `Error` on malformed input. Delegates to
/// `sidereon_core::rinex::crinex::encode_crinex`.
#[wasm_bindgen(js_name = encodeCrinex)]
pub fn encode_crinex(rinex_text: &str) -> Result<String, JsValue> {
    core_encode_crinex(rinex_text).map_err(engine_error)
}
