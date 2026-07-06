//! Position-error metrics from ENU/ECEF covariance inputs.
//!
//! The functions here validate and marshal caller-supplied covariance matrices,
//! then delegate all metric calculations to `sidereon_core::error_metrics`.

use std::collections::BTreeMap;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::error_metrics::{
    error_ellipse_from_enu_m2 as core_error_ellipse_from_enu_m2,
    horizontal_radius_at as core_horizontal_radius_at,
    metrics_from_ecef_covariance_m2 as core_metrics_from_ecef_covariance_m2,
    metrics_from_enu_covariance_m2 as core_metrics_from_enu_covariance_m2,
    metrics_from_kinematic_solution as core_metrics_from_kinematic_solution,
    metrics_from_position_covariance as core_metrics_from_position_covariance,
    spherical_radius_at as core_spherical_radius_at, vertical_radius_at as core_vertical_radius_at,
    ErrorEllipse as CoreErrorEllipse, ErrorMetricsError, PercentileRadius as CorePercentileRadius,
    PositionErrorMetrics as CorePositionErrorMetrics,
};
use sidereon_core::frame::Wgs84Geodetic;
use sidereon_core::geometry::PositionCovariance;
use sidereon_core::precise_positioning::{KinematicEpochSolution, KinematicEpochStatus};

use crate::error::{range_error, type_error};

fn metrics_error(error: ErrorMetricsError) -> JsValue {
    match error {
        ErrorMetricsError::NonFinite => range_error("covariance must contain only finite values"),
        ErrorMetricsError::NotPositiveSemidefinite => {
            range_error("covariance must be positive semidefinite")
        }
        ErrorMetricsError::InvalidProbability => range_error("probability must be in (0, 1)"),
        ErrorMetricsError::Rotation(error) => {
            range_error(&format!("ECEF covariance rotation failed: {error}"))
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverInput {
    lat_rad: f64,
    lon_rad: f64,
    #[serde(default)]
    height_m: Option<f64>,
}

impl ReceiverInput {
    fn to_core(&self) -> Result<Wgs84Geodetic, JsValue> {
        Wgs84Geodetic::new(self.lat_rad, self.lon_rad, self.height_m.unwrap_or(0.0))
            .map_err(|e| range_error(&e.to_string()))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KinematicSolutionInput {
    position_m: [f64; 3],
    position_covariance_m2: [[f64; 3]; 3],
    #[serde(default)]
    clock_m: Option<f64>,
    #[serde(default)]
    ztd_residual_m: Option<f64>,
    #[serde(default)]
    used_sats: Vec<String>,
    #[serde(default)]
    innovation_rms_m: Option<f64>,
}

impl KinematicSolutionInput {
    fn to_core(&self) -> KinematicEpochSolution {
        KinematicEpochSolution {
            position_m: self.position_m,
            clock_m: self.clock_m.unwrap_or(0.0),
            ztd_residual_m: self.ztd_residual_m.unwrap_or(0.0),
            ambiguities_m: BTreeMap::new(),
            position_covariance_m2: self.position_covariance_m2,
            used_sats: self.used_sats.clone(),
            innovation_rms_m: self.innovation_rms_m.unwrap_or(0.0),
            status: KinematicEpochStatus::Updated,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionCovarianceInput {
    ecef_m2: [[f64; 3]; 3],
    enu_m2: [[f64; 3]; 3],
}

impl PositionCovarianceInput {
    fn to_core(&self) -> PositionCovariance {
        PositionCovariance {
            ecef_m2: self.ecef_m2,
            enu_m2: self.enu_m2,
        }
    }
}

/// A horizontal one-sigma error ellipse.
#[wasm_bindgen]
pub struct ErrorEllipse {
    inner: CoreErrorEllipse,
}

#[wasm_bindgen]
impl ErrorEllipse {
    /// Semi-major axis length, metres.
    #[wasm_bindgen(getter, js_name = semiMajorM)]
    pub fn semi_major_m(&self) -> f64 {
        self.inner.semi_major_m
    }

    /// Semi-minor axis length, metres.
    #[wasm_bindgen(getter, js_name = semiMinorM)]
    pub fn semi_minor_m(&self) -> f64 {
        self.inner.semi_minor_m
    }

    /// Semi-major-axis orientation in radians, from east toward north.
    #[wasm_bindgen(getter, js_name = orientationRad)]
    pub fn orientation_rad(&self) -> f64 {
        self.inner.orientation_rad
    }
}

/// A percentile circle or sphere radius.
#[wasm_bindgen]
pub struct PercentileRadius {
    inner: CorePercentileRadius,
}

#[wasm_bindgen]
impl PercentileRadius {
    /// Probability mass inside this radius.
    #[wasm_bindgen(getter)]
    pub fn probability(&self) -> f64 {
        self.inner.probability
    }

    /// Exact circle or sphere radius, metres.
    #[wasm_bindgen(getter, js_name = radiusM)]
    pub fn radius_m(&self) -> f64 {
        self.inner.radius_m
    }

    /// Approximate named radius, metres, when applicable.
    #[wasm_bindgen(getter, js_name = approxM)]
    pub fn approx_m(&self) -> f64 {
        self.inner.approx_m
    }

    /// Whether `approxM` is valid for the covariance ratio.
    #[wasm_bindgen(getter, js_name = approxValid)]
    pub fn approx_valid(&self) -> bool {
        self.inner.approx_valid
    }
}

/// Standard position-error metrics derived from a position covariance.
#[wasm_bindgen]
pub struct PositionErrorMetrics {
    inner: CorePositionErrorMetrics,
}

#[wasm_bindgen]
impl PositionErrorMetrics {
    /// Horizontal one-sigma covariance ellipse.
    #[wasm_bindgen(getter)]
    pub fn ellipse(&self) -> ErrorEllipse {
        ErrorEllipse {
            inner: self.inner.ellipse,
        }
    }

    /// East standard deviation, metres.
    #[wasm_bindgen(getter, js_name = sigmaEM)]
    pub fn sigma_e_m(&self) -> f64 {
        self.inner.sigma_e_m
    }

    /// North standard deviation, metres.
    #[wasm_bindgen(getter, js_name = sigmaNM)]
    pub fn sigma_n_m(&self) -> f64 {
        self.inner.sigma_n_m
    }

    /// Up standard deviation, metres.
    #[wasm_bindgen(getter, js_name = sigmaUM)]
    pub fn sigma_u_m(&self) -> f64 {
        self.inner.sigma_u_m
    }

    /// Horizontal 50 percent circle radius.
    #[wasm_bindgen(getter, js_name = cepM)]
    pub fn cep_m(&self) -> PercentileRadius {
        PercentileRadius {
            inner: self.inner.cep_m,
        }
    }

    /// Horizontal 95 percent circle radius.
    #[wasm_bindgen(getter, js_name = r95M)]
    pub fn r95_m(&self) -> PercentileRadius {
        PercentileRadius {
            inner: self.inner.r95_m,
        }
    }

    /// Horizontal 99 percent circle radius.
    #[wasm_bindgen(getter, js_name = r99M)]
    pub fn r99_m(&self) -> PercentileRadius {
        PercentileRadius {
            inner: self.inner.r99_m,
        }
    }

    /// Distance root mean square, metres.
    #[wasm_bindgen(getter, js_name = drmsM)]
    pub fn drms_m(&self) -> f64 {
        self.inner.drms_m
    }

    /// Two times distance root mean square, metres.
    #[wasm_bindgen(getter, js_name = twoDrmsM)]
    pub fn two_drms_m(&self) -> f64 {
        self.inner.two_drms_m
    }

    /// Vertical 50 percent one-dimensional radius, metres.
    #[wasm_bindgen(getter, js_name = vepM)]
    pub fn vep_m(&self) -> f64 {
        self.inner.vep_m
    }

    /// Three-dimensional 50 percent sphere radius.
    #[wasm_bindgen(getter, js_name = sepM)]
    pub fn sep_m(&self) -> PercentileRadius {
        PercentileRadius {
            inner: self.inner.sep_m,
        }
    }

    /// Mean radial spherical error, metres.
    #[wasm_bindgen(getter, js_name = mrseM)]
    pub fn mrse_m(&self) -> f64 {
        self.inner.mrse_m
    }
}

/// Compute position-error metrics from an ENU covariance in square metres.
#[wasm_bindgen(js_name = metricsFromEnuCovarianceM2)]
pub fn metrics_from_enu_covariance_m2(
    covariance_enu_m2: JsValue,
) -> Result<PositionErrorMetrics, JsValue> {
    let covariance: [[f64; 3]; 3] = serde_wasm_bindgen::from_value(covariance_enu_m2)
        .map_err(|e| type_error(&format!("invalid ENU covariance: {e}")))?;
    Ok(PositionErrorMetrics {
        inner: core_metrics_from_enu_covariance_m2(covariance).map_err(metrics_error)?,
    })
}

/// Rotate an ECEF covariance to ENU and compute position-error metrics.
///
/// `receiver` is `{ latRad, lonRad, heightM? }` with radians and metres.
#[wasm_bindgen(js_name = metricsFromEcefCovarianceM2)]
pub fn metrics_from_ecef_covariance_m2(
    covariance_ecef_m2: JsValue,
    receiver: JsValue,
) -> Result<PositionErrorMetrics, JsValue> {
    let covariance: [[f64; 3]; 3] = serde_wasm_bindgen::from_value(covariance_ecef_m2)
        .map_err(|e| type_error(&format!("invalid ECEF covariance: {e}")))?;
    let receiver: ReceiverInput = serde_wasm_bindgen::from_value(receiver)
        .map_err(|e| type_error(&format!("invalid receiver: {e}")))?;
    Ok(PositionErrorMetrics {
        inner: core_metrics_from_ecef_covariance_m2(covariance, receiver.to_core()?)
            .map_err(metrics_error)?,
    })
}

/// Compute position-error metrics from a position covariance object.
///
/// The object must include `ecefM2` and `enuM2`, each a 3 by 3 covariance in
/// square metres.
#[wasm_bindgen(js_name = metricsFromPositionCovariance)]
pub fn metrics_from_position_covariance(
    covariance: JsValue,
) -> Result<PositionErrorMetrics, JsValue> {
    let covariance: PositionCovarianceInput = serde_wasm_bindgen::from_value(covariance)
        .map_err(|e| type_error(&format!("invalid position covariance: {e}")))?;
    Ok(PositionErrorMetrics {
        inner: core_metrics_from_position_covariance(&covariance.to_core())
            .map_err(metrics_error)?,
    })
}

/// Compute position-error metrics from a kinematic solution object.
///
/// The object must include `positionM` and `positionCovarianceM2`; optional
/// extra kinematic fields are accepted and ignored by this metric calculation.
#[wasm_bindgen(js_name = metricsFromKinematicSolution)]
pub fn metrics_from_kinematic_solution(solution: JsValue) -> Result<PositionErrorMetrics, JsValue> {
    let solution: KinematicSolutionInput = serde_wasm_bindgen::from_value(solution)
        .map_err(|e| type_error(&format!("invalid kinematic solution: {e}")))?;
    Ok(PositionErrorMetrics {
        inner: core_metrics_from_kinematic_solution(&solution.to_core()).map_err(metrics_error)?,
    })
}

/// Horizontal one-sigma ellipse from an ENU covariance in square metres.
#[wasm_bindgen(js_name = errorEllipseFromEnuM2)]
pub fn error_ellipse_from_enu_m2(covariance_enu_m2: JsValue) -> Result<ErrorEllipse, JsValue> {
    let covariance: [[f64; 3]; 3] = serde_wasm_bindgen::from_value(covariance_enu_m2)
        .map_err(|e| type_error(&format!("invalid ENU covariance: {e}")))?;
    Ok(ErrorEllipse {
        inner: core_error_ellipse_from_enu_m2(covariance).map_err(metrics_error)?,
    })
}

/// Horizontal percentile circle radius from an ENU covariance.
#[wasm_bindgen(js_name = horizontalRadiusAt)]
pub fn horizontal_radius_at(
    covariance_enu_m2: JsValue,
    probability: f64,
) -> Result<PercentileRadius, JsValue> {
    let covariance: [[f64; 3]; 3] = serde_wasm_bindgen::from_value(covariance_enu_m2)
        .map_err(|e| type_error(&format!("invalid ENU covariance: {e}")))?;
    Ok(PercentileRadius {
        inner: core_horizontal_radius_at(covariance, probability).map_err(metrics_error)?,
    })
}

/// Three-dimensional percentile sphere radius from an ENU covariance.
#[wasm_bindgen(js_name = sphericalRadiusAt)]
pub fn spherical_radius_at(
    covariance_enu_m2: JsValue,
    probability: f64,
) -> Result<PercentileRadius, JsValue> {
    let covariance: [[f64; 3]; 3] = serde_wasm_bindgen::from_value(covariance_enu_m2)
        .map_err(|e| type_error(&format!("invalid ENU covariance: {e}")))?;
    Ok(PercentileRadius {
        inner: core_spherical_radius_at(covariance, probability).map_err(metrics_error)?,
    })
}

/// Vertical one-dimensional percentile radius from an up variance.
#[wasm_bindgen(js_name = verticalRadiusAt)]
pub fn vertical_radius_at(sigma_u_m2: f64, probability: f64) -> Result<f64, JsValue> {
    core_vertical_radius_at(sigma_u_m2, probability).map_err(metrics_error)
}
