//! Uncertainty-aware WGS84 geofence binding.
//!
//! The core owns polygon construction, geodesic containment, signed boundary
//! distance, probability integration, and hysteresis. This module only decodes
//! JS inputs, keeps a reusable fence handle, and maps core failures to typed JS
//! `Error` objects with a `kind` property.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::error_metrics::PercentileRadius;
use sidereon_core::geofence::{
    containment, containment_probability_with_options, crossing_probability_with_options,
    distance_to_boundary as core_distance_to_boundary, CrossingKind as CoreCrossingKind,
    Fence as CoreFence, GeofenceError as CoreGeofenceError,
    GeofencePositionEstimate as CorePositionEstimate,
    PositionUncertainty as CorePositionUncertainty, ProbabilityHysteresis as CoreHysteresis,
    ProbabilityMethod as CoreProbabilityMethod, ProbabilityOptions as CoreProbabilityOptions,
};
use sidereon_core::Wgs84Geodetic;

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::mat3_from_flat;

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn attach_detail<T: Serialize>(value: &JsValue, detail: &T) {
    let detail_value =
        serde_wasm_bindgen::to_value(detail).expect("serialize typed geofence error detail");
    js_sys::Reflect::set(value, &JsValue::from_str("detail"), &detail_value)
        .expect("attach typed geofence error detail");
}

fn typed_error<T: Serialize>(name: &'static str, message: String, detail: &T) -> JsValue {
    let js_error = js_sys::Error::new(&message);
    js_error.set_name(name);
    let value: JsValue = js_error.into();
    js_sys::Reflect::set(&value, &JsValue::from_str("kind"), &JsValue::from_str(name))
        .expect("attach typed geofence error kind");
    attach_detail(&value, detail);
    value
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeofenceErrorDetail {
    name: &'static str,
    message: String,
}

fn geofence_error(error: CoreGeofenceError) -> JsValue {
    let kind = GeofenceError::from(&error);
    let name = geofence_error_name(kind);
    let detail = GeofenceErrorDetail {
        name,
        message: error.to_string(),
    };
    typed_error(name, detail.message.clone(), &detail)
}

fn geofence_error_name(error: GeofenceError) -> &'static str {
    match error {
        GeofenceError::TooFewVertices => "TooFewVertices",
        GeofenceError::InvalidInput => "InvalidInput",
        GeofenceError::Geodesic => "Geodesic",
        GeofenceError::Dop => "Dop",
        GeofenceError::ErrorMetrics => "ErrorMetrics",
    }
}

fn probability_method_name(method: GeofenceProbabilityMethod) -> &'static str {
    match method {
        GeofenceProbabilityMethod::BoundaryNormal => "boundaryNormal",
        GeofenceProbabilityMethod::PlanarQuadrature => "planarQuadrature",
    }
}

fn crossing_kind_name(kind: GeofenceCrossingKind) -> &'static str {
    match kind {
        GeofenceCrossingKind::Entered => "entered",
        GeofenceCrossingKind::Left => "left",
    }
}

fn geodetic(lat_rad: f64, lon_rad: f64, height_m: f64) -> Result<Wgs84Geodetic, JsValue> {
    Wgs84Geodetic::new(lat_rad, lon_rad, height_m).map_err(|e| range_error(&e.to_string()))
}

fn vertices_2d_from_flat(vertices: &[f64]) -> Result<Vec<Wgs84Geodetic>, JsValue> {
    if !vertices.len().is_multiple_of(2) {
        return Err(type_error("vertices must be flat [latRad, lonRad] rows"));
    }
    if vertices.is_empty() {
        return Err(type_error("vertices must contain at least one row"));
    }
    vertices
        .chunks_exact(2)
        .map(|row| geodetic(row[0], row[1], 0.0))
        .collect()
}

fn vertices_3d_from_flat(vertices: &[f64]) -> Result<Vec<Wgs84Geodetic>, JsValue> {
    if !vertices.len().is_multiple_of(3) {
        return Err(type_error(
            "vertices must be flat [latRad, lonRad, heightM] rows",
        ));
    }
    if vertices.is_empty() {
        return Err(type_error("vertices must contain at least one row"));
    }
    vertices
        .chunks_exact(3)
        .map(|row| geodetic(row[0], row[1], row[2]))
        .collect()
}

fn require_finite_matrix(name: &str, matrix: [[f64; 3]; 3]) -> Result<[[f64; 3]; 3], JsValue> {
    if matrix.iter().flatten().all(|value| value.is_finite()) {
        Ok(matrix)
    } else {
        Err(range_error(&format!(
            "{name} must contain only finite numbers"
        )))
    }
}

fn probability_options(value: JsValue) -> Result<CoreProbabilityOptions, JsValue> {
    let input: ProbabilityOptionsInput = if value.is_undefined() || value.is_null() {
        ProbabilityOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid geofence probability options: {e}")))?
    };
    Ok(CoreProbabilityOptions {
        method: match input.method.as_deref() {
            None | Some("boundaryNormal") | Some("boundary_normal") => {
                CoreProbabilityMethod::BoundaryNormal
            }
            Some("planarQuadrature") | Some("planar_quadrature") => {
                CoreProbabilityMethod::PlanarQuadrature
            }
            Some(other) => {
                return Err(type_error(&format!(
                    "invalid probability method {other:?}: expected \"boundaryNormal\" or \"planarQuadrature\""
                )))
            }
        },
    })
}

fn hysteresis(input: &ProbabilityOptionsInput) -> Result<CoreHysteresis, JsValue> {
    let defaults = CoreHysteresis::default();
    CoreHysteresis::new(
        input.enter_confidence.unwrap_or(defaults.enter_confidence),
        input.leave_confidence.unwrap_or(defaults.leave_confidence),
    )
    .map_err(geofence_error)
}

fn uncertainty(input: UncertaintyInput) -> Result<CorePositionUncertainty, JsValue> {
    match input.kind.as_deref() {
        Some("enuCovarianceM2") | Some("enu_covariance_m2") => {
            let matrix = input
                .covariance_m2
                .ok_or_else(|| type_error("uncertainty.covarianceM2 is required"))?;
            Ok(CorePositionUncertainty::EnuCovarianceM2(
                require_finite_matrix("uncertainty.covarianceM2", mat3_from_flat(
                    "uncertainty.covarianceM2",
                    &matrix,
                )?)?,
            ))
        }
        Some("ecefCovarianceM2") | Some("ecef_covariance_m2") => {
            let matrix = input
                .covariance_m2
                .ok_or_else(|| type_error("uncertainty.covarianceM2 is required"))?;
            Ok(CorePositionUncertainty::EcefCovarianceM2(
                require_finite_matrix("uncertainty.covarianceM2", mat3_from_flat(
                    "uncertainty.covarianceM2",
                    &matrix,
                )?)?,
            ))
        }
        Some("cepRadiusM") | Some("cep_radius_m") => {
            let radius_m = input
                .radius_m
                .ok_or_else(|| type_error("uncertainty.radiusM is required"))?;
            if radius_m.is_finite() && radius_m >= 0.0 {
                Ok(CorePositionUncertainty::CepRadiusM(radius_m))
            } else {
                Err(range_error("uncertainty.radiusM must be finite and non-negative"))
            }
        }
        Some("horizontalRadius") | Some("horizontal_radius") => {
            let radius_m = input
                .radius_m
                .ok_or_else(|| type_error("uncertainty.radiusM is required"))?;
            let probability = input
                .probability
                .ok_or_else(|| type_error("uncertainty.probability is required"))?;
            if !(radius_m.is_finite() && radius_m >= 0.0) {
                return Err(range_error(
                    "uncertainty.radiusM must be finite and non-negative",
                ));
            }
            if !(probability.is_finite() && probability > 0.0 && probability < 1.0) {
                return Err(range_error("uncertainty.probability must be in (0, 1)"));
            }
            Ok(CorePositionUncertainty::HorizontalRadius(
                PercentileRadius {
                    probability,
                    radius_m,
                    approx_m: radius_m,
                    approx_valid: true,
                },
            ))
        }
        None => Err(type_error("uncertainty.kind is required")),
        Some(other) => Err(type_error(&format!(
            "invalid uncertainty kind {other:?}: expected \"enuCovarianceM2\", \"ecefCovarianceM2\", \"cepRadiusM\", or \"horizontalRadius\""
        ))),
    }
}

fn crossing_kind_from_core(kind: CoreCrossingKind) -> GeofenceCrossingKind {
    match kind {
        CoreCrossingKind::Entered => GeofenceCrossingKind::Entered,
        CoreCrossingKind::Left => GeofenceCrossingKind::Left,
    }
}

/// Geofence construction and evaluation error variants.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeofenceError {
    /// Fewer than three distinct vertices were supplied.
    TooFewVertices,
    /// A geofence input value was outside its domain.
    InvalidInput,
    /// The geodesic direct or inverse calculation failed.
    Geodesic,
    /// ECEF covariance rotation failed.
    Dop,
    /// Covariance or radius validation failed.
    ErrorMetrics,
}

impl From<&CoreGeofenceError> for GeofenceError {
    fn from(value: &CoreGeofenceError) -> Self {
        match value {
            CoreGeofenceError::TooFewVertices => Self::TooFewVertices,
            CoreGeofenceError::InvalidInput { .. } => Self::InvalidInput,
            CoreGeofenceError::Geodesic(_) => Self::Geodesic,
            CoreGeofenceError::Dop(_) => Self::Dop,
            CoreGeofenceError::ErrorMetrics(_) => Self::ErrorMetrics,
        }
    }
}

/// Stable string label for a [`GeofenceError`] enum value.
#[wasm_bindgen(js_name = geofenceErrorLabel)]
pub fn geofence_error_label(error: GeofenceError) -> String {
    geofence_error_name(error).to_string()
}

/// Probability integration method for geofence uncertainty.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeofenceProbabilityMethod {
    /// Gaussian half-space approximation from boundary distance and normal variance.
    BoundaryNormal,
    /// Fixed quadrature over the local planarized fence.
    PlanarQuadrature,
}

/// Stable string label for a [`GeofenceProbabilityMethod`] enum value.
#[wasm_bindgen(js_name = geofenceProbabilityMethodLabel)]
pub fn geofence_probability_method_label(method: GeofenceProbabilityMethod) -> String {
    probability_method_name(method).to_string()
}

/// Geofence crossing event direction.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GeofenceCrossingKind {
    /// The sample sequence entered the fence.
    Entered,
    /// The sample sequence left the fence.
    Left,
}

/// Stable string label for a [`GeofenceCrossingKind`] enum value.
#[wasm_bindgen(js_name = geofenceCrossingKindLabel)]
pub fn geofence_crossing_kind_label(kind: GeofenceCrossingKind) -> String {
    crossing_kind_name(kind).to_string()
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ProbabilityOptionsInput {
    method: Option<String>,
    enter_confidence: Option<f64>,
    leave_confidence: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UncertaintyInput {
    kind: Option<String>,
    #[serde(default)]
    covariance_m2: Option<Vec<f64>>,
    #[serde(default)]
    radius_m: Option<f64>,
    #[serde(default)]
    probability: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionEstimateInput {
    lat_rad: f64,
    lon_rad: f64,
    #[serde(default)]
    height_m: f64,
    uncertainty: UncertaintyInput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CrossingEventJs {
    sample_index: usize,
    kind: &'static str,
    inside_probability: f64,
}

/// A geodesic polygon fence on WGS84.
#[wasm_bindgen]
pub struct Geofence {
    inner: CoreFence,
}

#[wasm_bindgen]
impl Geofence {
    /// Construct a geodesic WGS84 polygon from flat 2D radian vertices.
    ///
    /// `vertices` is `[latRad, lonRad, ...]`. Use `geofenceFromVertices3d`
    /// for `[latRad, lonRad, heightM, ...]` rows.
    #[wasm_bindgen(constructor)]
    pub fn new(vertices: &[f64]) -> Result<Geofence, JsValue> {
        let vertices = vertices_2d_from_flat(vertices)?;
        let inner = CoreFence::new(vertices).map_err(geofence_error)?;
        Ok(Self { inner })
    }

    /// Number of polygon edges.
    #[wasm_bindgen(getter, js_name = edgeCount)]
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Fence vertices in open-polygon form as flat `[latRad, lonRad, heightM]` rows.
    #[wasm_bindgen(getter)]
    pub fn vertices(&self) -> Vec<f64> {
        self.inner
            .vertices()
            .iter()
            .flat_map(|v| [v.lat_rad, v.lon_rad, v.height_m])
            .collect()
    }

    /// Whether the core small-region planar path can evaluate this position.
    #[wasm_bindgen(js_name = planarFastPathApplies)]
    pub fn planar_fast_path_applies(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        height_m: f64,
    ) -> Result<bool, JsValue> {
        Ok(self
            .inner
            .planar_fast_path_applies(geodetic(lat_rad, lon_rad, height_m)?))
    }

    /// Boolean containment for one WGS84 geodetic position.
    pub fn contains(&self, lat_rad: f64, lon_rad: f64, height_m: f64) -> Result<bool, JsValue> {
        containment(geodetic(lat_rad, lon_rad, height_m)?, &self.inner).map_err(geofence_error)
    }

    /// Signed distance to the fence boundary in metres.
    #[wasm_bindgen(js_name = distanceToBoundary)]
    pub fn distance_to_boundary(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        height_m: f64,
    ) -> Result<f64, JsValue> {
        core_distance_to_boundary(geodetic(lat_rad, lon_rad, height_m)?, &self.inner)
            .map_err(geofence_error)
    }

    /// Containment probability for one position and uncertainty object.
    ///
    /// `uncertainty.kind` is one of `"enuCovarianceM2"`, `"ecefCovarianceM2"`,
    /// `"cepRadiusM"`, or `"horizontalRadius"`. `options.method` is
    /// `"boundaryNormal"` or `"planarQuadrature"`.
    #[wasm_bindgen(js_name = containmentProbability)]
    pub fn containment_probability(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        height_m: f64,
        uncertainty_value: JsValue,
        options: JsValue,
    ) -> Result<f64, JsValue> {
        let position = geodetic(lat_rad, lon_rad, height_m)?;
        let uncertainty_input: UncertaintyInput = serde_wasm_bindgen::from_value(uncertainty_value)
            .map_err(|e| type_error(&format!("invalid geofence uncertainty: {e}")))?;
        let uncertainty = uncertainty(uncertainty_input)?;
        let options = probability_options(options)?;
        containment_probability_with_options(position, uncertainty, &self.inner, options)
            .map_err(geofence_error)
    }

    /// Probabilistic crossing detection over geodetic position estimates.
    ///
    /// `samples` is an array of `{ latRad, lonRad, heightM?, uncertainty }`.
    /// `options.enterConfidence` and `options.leaveConfidence` configure
    /// hysteresis; absent values use the core defaults.
    #[wasm_bindgen(js_name = crossingProbability)]
    pub fn crossing_probability(
        &self,
        samples: JsValue,
        options: JsValue,
    ) -> Result<JsValue, JsValue> {
        let sample_inputs: Vec<PositionEstimateInput> = serde_wasm_bindgen::from_value(samples)
            .map_err(|e| type_error(&format!("invalid geofence samples: {e}")))?;
        let option_input: ProbabilityOptionsInput = if options.is_undefined() || options.is_null() {
            ProbabilityOptionsInput::default()
        } else {
            serde_wasm_bindgen::from_value(options.clone())
                .map_err(|e| type_error(&format!("invalid geofence crossing options: {e}")))?
        };
        let probability_options = probability_options(options)?;
        let hysteresis = hysteresis(&option_input)?;
        let core_samples = sample_inputs
            .into_iter()
            .map(|sample| {
                Ok(CorePositionEstimate {
                    position: geodetic(sample.lat_rad, sample.lon_rad, sample.height_m)?,
                    uncertainty: uncertainty(sample.uncertainty)?,
                })
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let events = crossing_probability_with_options(
            &core_samples,
            &self.inner,
            hysteresis,
            probability_options,
        )
        .map_err(geofence_error)?;
        let out: Vec<CrossingEventJs> = events
            .into_iter()
            .map(|event| {
                let kind = crossing_kind_from_core(event.kind);
                CrossingEventJs {
                    sample_index: event.sample_index,
                    kind: crossing_kind_name(kind),
                    inside_probability: event.inside_probability,
                }
            })
            .collect();
        to_js(&out)
    }
}

/// Construct a geodesic WGS84 polygon from flat 2D radian vertices.
#[wasm_bindgen(js_name = geofenceFromVertices)]
pub fn geofence_from_vertices(vertices: &[f64]) -> Result<Geofence, JsValue> {
    Geofence::new(vertices)
}

/// Construct a geodesic WGS84 polygon from flat 3D radian vertices.
///
/// `vertices` is `[latRad, lonRad, heightM, ...]`. Heights are accepted but
/// ignored by the core geofence model.
#[wasm_bindgen(js_name = geofenceFromVertices3d)]
pub fn geofence_from_vertices_3d(vertices: &[f64]) -> Result<Geofence, JsValue> {
    let vertices = vertices_3d_from_flat(vertices)?;
    let inner = CoreFence::new(vertices).map_err(geofence_error)?;
    Ok(Geofence { inner })
}

/// Boolean containment for one position and flat vertex array.
#[wasm_bindgen(js_name = geofenceContains)]
pub fn geofence_contains(
    vertices: &[f64],
    lat_rad: f64,
    lon_rad: f64,
    height_m: f64,
) -> Result<bool, JsValue> {
    Geofence::new(vertices)?.contains(lat_rad, lon_rad, height_m)
}

/// Containment probability using default probability options.
#[wasm_bindgen(js_name = geofenceContainmentProbability)]
pub fn geofence_containment_probability(
    vertices: &[f64],
    lat_rad: f64,
    lon_rad: f64,
    height_m: f64,
    uncertainty_value: JsValue,
) -> Result<f64, JsValue> {
    Geofence::new(vertices)?.containment_probability(
        lat_rad,
        lon_rad,
        height_m,
        uncertainty_value,
        JsValue::UNDEFINED,
    )
}

/// Probabilistic crossing detection with default hysteresis.
#[wasm_bindgen(js_name = geofenceCrossingProbability)]
pub fn geofence_crossing_probability(
    vertices: &[f64],
    samples: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    Geofence::new(vertices)?.crossing_probability(samples, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fence() -> Geofence {
        Geofence::new(&[
            -0.01_f64.to_radians(),
            0.0,
            -0.01_f64.to_radians(),
            0.02_f64.to_radians(),
            0.01_f64.to_radians(),
            0.02_f64.to_radians(),
            0.01_f64.to_radians(),
            0.0,
        ])
        .expect("fence builds")
    }

    #[test]
    fn labels_track_enums() {
        assert_eq!(
            geofence_probability_method_label(GeofenceProbabilityMethod::BoundaryNormal),
            "boundaryNormal"
        );
        assert_eq!(
            geofence_crossing_kind_label(GeofenceCrossingKind::Entered),
            "entered"
        );
        assert_eq!(
            geofence_error_label(GeofenceError::TooFewVertices),
            "TooFewVertices"
        );
    }

    #[test]
    fn flat_vertices_construct_core_fence() {
        let fence = fence();
        assert_eq!(fence.edge_count(), 4);
    }
}
