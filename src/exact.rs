//! Exact-length serde adapters for fixed-size public numeric inputs.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{Error as _, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use wasm_bindgen::JsValue;

use crate::error::type_error;

#[derive(Clone, Copy)]
struct ExactVec3([f64; 3]);

impl<'de> Deserialize<'de> for ExactVec3 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Vec3Visitor;

        impl<'de> Visitor<'de> for Vec3Visitor {
            type Value = ExactVec3;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an array of length 3")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = [0.0; 3];
                for (index, value) in values.iter_mut().enumerate() {
                    *value = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(index, &self))?;
                }
                if seq.next_element::<IgnoredAny>()?.is_some() {
                    return Err(A::Error::invalid_length(4, &self));
                }
                Ok(ExactVec3(values))
            }
        }

        deserializer.deserialize_seq(Vec3Visitor)
    }
}

#[derive(Clone, Copy)]
struct ExactMat3([[f64; 3]; 3]);

impl<'de> Deserialize<'de> for ExactMat3 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Mat3Visitor;

        impl<'de> Visitor<'de> for Mat3Visitor {
            type Value = ExactMat3;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an array of exactly 3 rows of exactly 3 numbers")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut rows = [[0.0; 3]; 3];
                for (index, row) in rows.iter_mut().enumerate() {
                    *row = seq
                        .next_element::<ExactVec3>()?
                        .ok_or_else(|| A::Error::invalid_length(index, &self))?
                        .0;
                }
                if seq.next_element::<IgnoredAny>()?.is_some() {
                    return Err(A::Error::invalid_length(4, &self));
                }
                Ok(ExactMat3(rows))
            }
        }

        deserializer.deserialize_seq(Mat3Visitor)
    }
}

pub fn vec3<'de, D>(deserializer: D) -> Result<[f64; 3], D::Error>
where
    D: Deserializer<'de>,
{
    Ok(ExactVec3::deserialize(deserializer)?.0)
}

pub fn option_vec3<'de, D>(deserializer: D) -> Result<Option<[f64; 3]>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<ExactVec3>::deserialize(deserializer)?.map(|value| value.0))
}

pub fn map_vec3<'de, D>(deserializer: D) -> Result<BTreeMap<String, [f64; 3]>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(BTreeMap::<String, ExactVec3>::deserialize(deserializer)?
        .into_iter()
        .map(|(key, value)| (key, value.0))
        .collect())
}

pub fn mat3<'de, D>(deserializer: D) -> Result<[[f64; 3]; 3], D::Error>
where
    D: Deserializer<'de>,
{
    Ok(ExactMat3::deserialize(deserializer)?.0)
}

pub fn option_mat3<'de, D>(deserializer: D) -> Result<Option<[[f64; 3]; 3]>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<ExactMat3>::deserialize(deserializer)?.map(|value| value.0))
}

pub fn mat3_from_js(value: JsValue) -> Result<[[f64; 3]; 3], JsValue> {
    serde_wasm_bindgen::from_value::<ExactMat3>(value)
        .map(|matrix| matrix.0)
        .map_err(|error| type_error(&format!("invalid 3-by-3 matrix: {error}")))
}
