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

/// Read an exact unsigned 64-bit integer from a JavaScript `bigint`.
///
/// Keeping the public parameter as `JsValue` lets the binding reject negative
/// and overflowing values instead of applying the wrapping conversion used by
/// wasm-bindgen's direct `u64` ABI.
pub fn u64_bigint(value: JsValue, field: &str) -> Result<u64, JsValue> {
    if !value.is_bigint() {
        return Err(type_error(&format!("{field} must be a bigint")));
    }
    serde_wasm_bindgen::from_value(value).map_err(|_| {
        range_error(&format!(
            "{field} must be between 0n and 18446744073709551615n"
        ))
    })
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

/// Describe a value JavaScript threw or returned, for an error message.
///
/// A thrown value need not be an `Error`; a string, a number and a plain object
/// all reach here, so the text falls back through the shapes in turn.
pub fn describe_js(value: &JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }
    if let Ok(message) = js_sys::Reflect::get(value, &JsValue::from_str("message")) {
        if let Some(text) = message.as_string() {
            return text;
        }
    }
    format!("{value:?}")
}

/// Serialize to plain JavaScript objects and arrays, never `Map`s, with
/// `Option::None` as `null`, so a caller reads a structured result with
/// ordinary property access and tells an absent value from a present one.
///
/// A value serde cannot represent, such as an integer past
/// `Number.MAX_SAFE_INTEGER`, is refused as an `Error` naming `what` rather
/// than rounded.
pub fn to_plain_js<T: serde::Serialize>(value: &T, what: &str) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|err| engine_error(format!("failed to serialize {what}: {err}")))
}

/// An `Error` whose `name` is `name`, whose `message` is `message`, and whose
/// `detail` property holds `detail` as a plain object.
///
/// The typed detail is part of the public contract, so a serialization or
/// property-set failure surfaces as its own error naming the original message
/// rather than as a bare exception with the detail silently missing.
pub fn error_with_detail<T: serde::Serialize>(name: &str, message: &str, detail: &T) -> JsValue {
    let detail_value = match to_plain_js(detail, "the typed error detail") {
        Ok(value) => value,
        Err(err) => {
            return engine_error(format!(
                "{message} (the typed detail could not be serialized: {})",
                describe_js(&err)
            ))
        }
    };
    let value: JsValue = match name {
        "RangeError" => js_sys::RangeError::new(message).into(),
        "TypeError" => js_sys::TypeError::new(message).into(),
        _ => {
            let js_error = js_sys::Error::new(message);
            js_error.set_name(name);
            js_error.into()
        }
    };
    match js_sys::Reflect::set(&value, &JsValue::from_str("detail"), &detail_value) {
        Ok(true) => value,
        Ok(false) => engine_error(format!(
            "{message} (the typed detail could not be attached to the error)"
        )),
        Err(err) => engine_error(format!(
            "{message} (attaching the typed detail threw: {})",
            describe_js(&err)
        )),
    }
}

/// A plain object holding each `(key, value)` pair, for a result that pairs a
/// class instance with plain diagnostics.
///
/// A property that cannot be set is refused as an `Error` naming it and `what`,
/// never left off the object.
pub fn result_object(entries: &[(&str, &JsValue)], what: &str) -> Result<JsValue, JsValue> {
    let result = js_sys::Object::new();
    for (key, value) in entries {
        match js_sys::Reflect::set(&result, &JsValue::from_str(key), value) {
            Ok(true) => {}
            Ok(false) => {
                return Err(engine_error(format!("failed to set '{key}' on the {what}")));
            }
            Err(err) => {
                return Err(engine_error(format!(
                    "setting '{key}' on the {what} threw: {}",
                    describe_js(&err)
                )));
            }
        }
    }
    Ok(result.into())
}

/// Largest index a wasm32 `usize` holds.
const MAX_WASM_INDEX: f64 = u32::MAX as f64;

/// Read a zero-based index from a JavaScript `number` exactly.
///
/// wasm-bindgen converts a `number` argument to `usize` with the `>>> 0`
/// wrapping cast, so `-1` becomes 4294967295 and `1.5` becomes 1. Taking the
/// argument as `f64` and checking it here refuses a negative, fractional,
/// non-finite or too-large value as a `RangeError` naming `field` instead of
/// reading a different index than the caller asked for.
pub fn index_arg(value: f64, field: &str) -> Result<usize, JsValue> {
    if !value.is_finite() {
        return Err(range_error(&format!("{field} must be a finite number")));
    }
    if value.fract() != 0.0 {
        return Err(range_error(&format!("{field} must be an integer")));
    }
    if !(0.0..=MAX_WASM_INDEX).contains(&value) {
        return Err(range_error(&format!(
            "{field} must lie within [0, {}]",
            u32::MAX
        )));
    }
    // Exact: an integer in [0, 2^32 - 1] converts without loss on wasm32 and
    // on every wider target.
    Ok(value as usize)
}

/// Refuse an unknown own property on a supplied input object.
///
/// `serde_wasm_bindgen` 0.6.5 deserializes a struct by asking the object for
/// each declared field in turn, so a property the struct does not declare is
/// never offered to serde and `deny_unknown_fields` would be inert. Without
/// this walk a misspelt key reads as an absent field.
///
/// `Object::getOwnPropertyNames` lists every own string property name,
/// enumerable or not, and does not invoke an accessor. A wasm-bindgen class
/// instance carries its pointer as the own property `__wbg_ptr`; that name is
/// not a field and is skipped, so an instance whose getters sit on its
/// prototype is read like a plain object. An inherited
/// property is not refused: serde reads fields through the prototype chain.
pub fn reject_unknown_keys(
    object: &JsValue,
    container: &str,
    known: &[&str],
) -> Result<(), JsValue> {
    use wasm_bindgen::JsCast;
    if !object.is_object() {
        return Err(type_error(&format!("{container} must be an object")));
    }
    let names = js_sys::Object::get_own_property_names(object.unchecked_ref::<js_sys::Object>());
    for index in 0..names.length() {
        let Some(key) = names.get(index).as_string() else {
            continue;
        };
        if key == "__wbg_ptr" {
            continue;
        }
        if !known.contains(&key.as_str()) {
            return Err(type_error(&format!(
                "unknown {container} property '{key}'; expected one of {}",
                known.join(", ")
            )));
        }
    }
    Ok(())
}

/// A `number` for an exact integer, or `None` where the integer would not
/// survive the conversion to a JavaScript `number`.
pub fn safe_integer_number(value: i128) -> Option<f64> {
    const MAX_SAFE: i128 = 9_007_199_254_740_991;
    (-MAX_SAFE..=MAX_SAFE)
        .contains(&value)
        .then_some(value as f64)
}

/// Read a UT1 validity policy: `"strict"` (the default) refuses an instant
/// outside the UT1 table; `"permissive"` accepts it and reports the departure.
pub fn ut1_validity(
    label: Option<String>,
) -> Result<sidereon_core::astro::time::ValidityMode, JsValue> {
    use sidereon_core::astro::time::ValidityMode;
    match label.as_deref() {
        None | Some("strict") => Ok(ValidityMode::Strict),
        Some("permissive") => Ok(ValidityMode::Permissive),
        Some(other) => Err(type_error(&format!(
            "invalid UT1 validity {other:?}: expected \"strict\" or \"permissive\""
        ))),
    }
}

/// `{ value, ut1Degraded }`: a result computed under a UT1 validity policy and
/// the departure from the UT1 table it accepted (`"beforeCoverage"`,
/// `"afterCoverage"`, or `null` when every instant lay inside the table).
pub fn validated_object(
    value: &JsValue,
    degraded: Option<sidereon_core::astro::time::DegradeReason>,
) -> Result<JsValue, JsValue> {
    let degraded = match degraded {
        Some(reason) => JsValue::from_str(crate::spp::degrade_reason_label(reason)),
        None => JsValue::NULL,
    };
    result_object(
        &[("value", value), ("ut1Degraded", &degraded)],
        "UT1-validated result",
    )
}
