//! Labels for engine enum variants this binding does not name yet.
//!
//! Several engine enums are `#[non_exhaustive]`, so a later engine release can
//! add a variant this binding has no label for. Such a variant crosses the
//! boundary under its own name, taken from its `Debug` form and restated in the
//! case the surrounding labels use, never as a catch-all that hides which
//! variant it was.

use std::borrow::Cow;
use std::fmt::Debug;

/// A label: a name this binding states, or one derived from a variant's
/// `Debug` form.
pub(crate) type Label = Cow<'static, str>;

/// The variant name `Debug` states for `value`: the text before the first `(`,
/// `{` or blank.
fn variant_name<T: Debug + ?Sized>(value: &T) -> String {
    let debug = format!("{value:?}");
    debug
        .split(|c: char| c == '(' || c == '{' || c.is_whitespace())
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Split a `PascalCase` name into its words: a new word starts at an upper-case
/// letter after a lower-case letter or digit, and at the last upper-case letter
/// of a run that a lower-case letter follows (`SSRRecords` is `SSR`,
/// `Records`).
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for (index, &c) in chars.iter().enumerate() {
        let boundary = index > 0
            && c.is_uppercase()
            && (!chars[index - 1].is_uppercase()
                || chars.get(index + 1).is_some_and(|next| next.is_lowercase()));
        if boundary && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The variant's name in lowerCamelCase (`SsrRecordsShort` is
/// `ssrRecordsShort`).
pub(crate) fn lower_camel_variant<T: Debug + ?Sized>(value: &T) -> Label {
    let mut out = String::new();
    for (index, word) in words(&variant_name(value)).iter().enumerate() {
        let lower = word.to_lowercase();
        if index == 0 {
            out.push_str(&lower);
        } else {
            let mut chars = lower.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    Cow::Owned(out)
}

/// The variant's name in UPPER_SNAKE_CASE (`NegativeZero` is
/// `NEGATIVE_ZERO`).
pub(crate) fn upper_snake_variant<T: Debug + ?Sized>(value: &T) -> Label {
    Cow::Owned(
        words(&variant_name(value))
            .iter()
            .map(|word| word.to_uppercase())
            .collect::<Vec<_>>()
            .join("_"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    enum Sample {
        SsrRecordsShort { _n: u8 },
        SSRRecords,
        NegativeZero,
        V300,
    }

    #[test]
    fn variant_names_restate_in_each_case() {
        assert_eq!(
            lower_camel_variant(&Sample::SsrRecordsShort { _n: 1 }),
            "ssrRecordsShort"
        );
        assert_eq!(lower_camel_variant(&Sample::SSRRecords), "ssrRecords");
        assert_eq!(lower_camel_variant(&Sample::NegativeZero), "negativeZero");
        assert_eq!(lower_camel_variant(&Sample::V300), "v300");
        assert_eq!(upper_snake_variant(&Sample::NegativeZero), "NEGATIVE_ZERO");
        assert_eq!(upper_snake_variant(&Sample::SSRRecords), "SSR_RECORDS");
        assert_eq!(upper_snake_variant(&Sample::V300), "V300");
    }
}
