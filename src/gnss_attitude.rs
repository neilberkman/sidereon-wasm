//! GNSS attitude laws, block identifiers and instantaneous geometry.

use std::sync::Arc;

use serde_json::json;
use sidereon_core::astro::attitude::gnss::{self, AttitudeTrajectory};
use sidereon_core::astro::de::DeEphemeris as CoreDeEphemeris;
use sidereon_core::astro::frames::{EarthOrientationProvider, TdbEarthOrientationProvider};
use sidereon_core::ephemeris;
use sidereon_core::orbit_scoring::{OrbitScoreError, DEFAULT_VELOCITY_HALF_STEP_S};
use sidereon_core::GnssSatelliteId;
use wasm_bindgen::prelude::*;

use crate::analytic::{BrouwerLyddanePropagator, EcksteinHechlerPropagator};
use crate::de::DeEphemeris;
use crate::earth_orientation::Iers2010EarthOrientationProvider;
use crate::error::{index_arg, range_error, typed_error, typed_error_with_kind};
use crate::gnss::GnssSystem;
use crate::marshal::vec3;
use crate::orbit_scoring::{score_error, OrbitSource};
use crate::propagation::DenseOutput;
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::{parse_sat, Sp3};

type JsResult<T> = Result<T, JsValue>;

fn parse_prn(prn: f64) -> JsResult<u8> {
    u8::try_from(index_arg(prn, "prn")?)
        .map_err(|_| range_error("prn must be an integer from 0 through 255"))
}

fn parse_manufacturer(value: f64) -> JsResult<gnss::BeidouManufacturer> {
    const VALUES: [gnss::BeidouManufacturer; 2] = [
        gnss::BeidouManufacturer::Cast,
        gnss::BeidouManufacturer::Secm,
    ];
    VALUES
        .get(index_arg(value, "manufacturer")?)
        .copied()
        .ok_or_else(|| range_error("manufacturer must be a BeidouManufacturer value"))
}

fn parse_beidou_orbit(value: f64) -> JsResult<gnss::BeidouOrbitClass> {
    const VALUES: [gnss::BeidouOrbitClass; 3] = [
        gnss::BeidouOrbitClass::Geo,
        gnss::BeidouOrbitClass::Igso,
        gnss::BeidouOrbitClass::Meo,
    ];
    VALUES
        .get(index_arg(value, "beidouOrbit")?)
        .copied()
        .ok_or_else(|| range_error("beidouOrbit must be a BeidouOrbitClass value"))
}

fn parse_start_angle(value: f64) -> JsResult<CosineTurnStartAngle> {
    const VALUES: [CosineTurnStartAngle; 2] = [
        CosineTurnStartAngle::Colinearity,
        CosineTurnStartAngle::OrbitAngle,
    ];
    VALUES
        .get(index_arg(value, "startAngle")?)
        .copied()
        .ok_or_else(|| range_error("startAngle must be a CosineTurnStartAngle value"))
}

pub(crate) fn parse_system(value: f64) -> JsResult<GnssSystem> {
    const VALUES: [GnssSystem; 7] = [
        GnssSystem::Gps,
        GnssSystem::Glonass,
        GnssSystem::Galileo,
        GnssSystem::BeiDou,
        GnssSystem::Qzss,
        GnssSystem::Navic,
        GnssSystem::Sbas,
    ];
    VALUES
        .get(index_arg(value, "system")?)
        .copied()
        .ok_or_else(|| range_error("system must be a GnssSystem value"))
}

fn attitude_error(error: gnss::GnssAttitudeError) -> JsValue {
    let detail = match &error {
        gnss::GnssAttitudeError::NonFinite { field } => json!({"kind":"nonFinite", "field":field}),
        gnss::GnssAttitudeError::ZeroLength { field } => {
            json!({"kind":"zeroLength", "field":field})
        }
        gnss::GnssAttitudeError::TrajectoryUnavailable { t_s } => {
            json!({"kind":"trajectoryUnavailable", "tS":t_s})
        }
        gnss::GnssAttitudeError::InvalidParameter { field, value } => {
            #[derive(serde::Serialize)]
            struct Detail<'a> {
                kind: &'static str,
                field: &'a str,
                value: f64,
            }

            let detail = Detail {
                kind: "invalidParameter",
                field,
                value: *value,
            };
            return typed_error_with_kind(
                "GnssAttitudeError",
                &error.to_string(),
                detail.kind,
                &detail,
            );
        }
        gnss::GnssAttitudeError::NoDefaultYawRate { prn } => {
            json!({"kind":"noDefaultYawRate", "prn":prn})
        }
        gnss::GnssAttitudeError::DegenerateGeometry { t_s } => {
            json!({"kind":"degenerateGeometry", "tS":t_s})
        }
        gnss::GnssAttitudeError::BeidouManufacturerRequired { t_s } => {
            json!({"kind":"beidouManufacturerRequired", "tS":t_s})
        }
        gnss::GnssAttitudeError::TurnEndNotFound { t_s } => {
            json!({"kind":"turnEndNotFound", "tS":t_s})
        }
        gnss::GnssAttitudeError::OutsideValidatedDomain { t_s, reason } => {
            json!({"kind":"outsideValidatedDomain", "tS":t_s, "reason":reason})
        }
        _ => json!({"kind":"unknown"}),
    };
    typed_error("GnssAttitudeError", &error.to_string(), detail)
}
fn block_error(error: gnss::GnssBlockError) -> JsValue {
    let detail = match &error {
        gnss::GnssBlockError::Unknown { name } => json!({"kind":"unknown", "name":name}),
        gnss::GnssBlockError::Unsupported { name } => json!({"kind":"unsupported", "name":name}),
        gnss::GnssBlockError::BeidouOrbitClassRequired { block_type } => {
            json!({"kind":"beidouOrbitClassRequired", "blockType":block_type})
        }
        _ => json!({"kind":"unknownVariant"}),
    };
    typed_error("GnssBlockError", &error.to_string(), detail)
}
/// Part of the yaw law that produced the attitude.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttitudeMode {
    /// Nominal yaw steering.
    Nominal,
    /// A turn at orbit noon.
    NoonTurn,
    /// A turn at orbit midnight outside a shadow-crossing law.
    MidnightTurn,
    /// GPS IIA post-shadow recovery.
    Shadow,
    /// Orbit-normal mode.
    OrbitNormal,
    /// Yaw steering with a modified Sun elevation (BeiDou-3 Secm).
    ModifiedYawSteering,
}

impl From<AttitudeMode> for gnss::AttitudeMode {
    fn from(value: AttitudeMode) -> Self {
        match value {
            AttitudeMode::Nominal => Self::Nominal,
            AttitudeMode::NoonTurn => Self::NoonTurn,
            AttitudeMode::MidnightTurn => Self::MidnightTurn,
            AttitudeMode::Shadow => Self::Shadow,
            AttitudeMode::OrbitNormal => Self::OrbitNormal,
            AttitudeMode::ModifiedYawSteering => Self::ModifiedYawSteering,
        }
    }
}

impl TryFrom<gnss::AttitudeMode> for AttitudeMode {
    type Error = JsValue;

    fn try_from(value: gnss::AttitudeMode) -> JsResult<Self> {
        match value {
            gnss::AttitudeMode::Nominal => Ok(Self::Nominal),
            gnss::AttitudeMode::NoonTurn => Ok(Self::NoonTurn),
            gnss::AttitudeMode::MidnightTurn => Ok(Self::MidnightTurn),
            gnss::AttitudeMode::Shadow => Ok(Self::Shadow),
            gnss::AttitudeMode::OrbitNormal => Ok(Self::OrbitNormal),
            gnss::AttitudeMode::ModifiedYawSteering => Ok(Self::ModifiedYawSteering),
            _ => Err(range_error("unsupported AttitudeMode value")),
        }
    }
}

/// Angle whose crossing starts a cosine yaw turn.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum CosineTurnStartAngle {
    /// `cos(beta) cos(in-plane angle)`.
    Colinearity,
    /// The in-plane orbit angle from noon or midnight (Wang et al. 2018).
    OrbitAngle,
}

impl From<CosineTurnStartAngle> for gnss::CosineTurnStartAngle {
    fn from(value: CosineTurnStartAngle) -> Self {
        match value {
            CosineTurnStartAngle::Colinearity => Self::Colinearity,
            CosineTurnStartAngle::OrbitAngle => Self::OrbitAngle,
        }
    }
}

impl TryFrom<gnss::CosineTurnStartAngle> for CosineTurnStartAngle {
    type Error = JsValue;

    fn try_from(value: gnss::CosineTurnStartAngle) -> JsResult<Self> {
        match value {
            gnss::CosineTurnStartAngle::Colinearity => Ok(Self::Colinearity),
            gnss::CosineTurnStartAngle::OrbitAngle => Ok(Self::OrbitAngle),
            _ => Err(range_error("unsupported CosineTurnStartAngle value")),
        }
    }
}

/// Manufacturer of a BeiDou-3 satellite.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BeidouManufacturer {
    /// China Academy of Space Technology.
    Cast,
    /// Shanghai Engineering Center for Microsatellites.
    Secm,
}

impl From<BeidouManufacturer> for gnss::BeidouManufacturer {
    fn from(value: BeidouManufacturer) -> Self {
        match value {
            BeidouManufacturer::Cast => Self::Cast,
            BeidouManufacturer::Secm => Self::Secm,
        }
    }
}

impl From<gnss::BeidouManufacturer> for BeidouManufacturer {
    fn from(value: gnss::BeidouManufacturer) -> Self {
        match value {
            gnss::BeidouManufacturer::Cast => Self::Cast,
            gnss::BeidouManufacturer::Secm => Self::Secm,
        }
    }
}

/// Orbit class of a BeiDou satellite.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BeidouOrbitClass {
    /// Geostationary orbit.
    Geo,
    /// Inclined geosynchronous orbit.
    Igso,
    /// Medium Earth orbit.
    Meo,
}

impl From<BeidouOrbitClass> for gnss::BeidouOrbitClass {
    fn from(value: BeidouOrbitClass) -> Self {
        match value {
            BeidouOrbitClass::Geo => Self::Geo,
            BeidouOrbitClass::Igso => Self::Igso,
            BeidouOrbitClass::Meo => Self::Meo,
        }
    }
}

impl From<gnss::BeidouOrbitClass> for BeidouOrbitClass {
    fn from(value: gnss::BeidouOrbitClass) -> Self {
        match value {
            gnss::BeidouOrbitClass::Geo => Self::Geo,
            gnss::BeidouOrbitClass::Igso => Self::Igso,
            gnss::BeidouOrbitClass::Meo => Self::Meo,
        }
    }
}

/// A GNSS satellite block with a published attitude law.
#[wasm_bindgen]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GnssBlock(gnss::GnssBlock);

#[wasm_bindgen]
impl GnssBlock {
    /// Compare the stored value.
    pub fn equals(&self, other: &GnssBlock) -> bool {
        self.0 == other.0
    }

    /// GPS IIA satellite block.
    #[wasm_bindgen(js_name = GPS_IIA)]
    pub fn gps_iia() -> Self {
        Self(gnss::GnssBlock::GpsIIA)
    }
    /// GPS IIR satellite block.
    #[wasm_bindgen(js_name = GPS_IIR)]
    pub fn gps_iir() -> Self {
        Self(gnss::GnssBlock::GpsIIR)
    }
    /// GPS IIR-M satellite block.
    #[wasm_bindgen(js_name = GPS_IIRM)]
    pub fn gps_iirm() -> Self {
        Self(gnss::GnssBlock::GpsIIRM)
    }
    /// GPS IIF satellite block.
    #[wasm_bindgen(js_name = GPS_IIF)]
    pub fn gps_iif() -> Self {
        Self(gnss::GnssBlock::GpsIIF)
    }
    /// GPS III satellite block.
    #[wasm_bindgen(js_name = GPS_III)]
    pub fn gps_iii() -> Self {
        Self(gnss::GnssBlock::GpsIII)
    }
    /// GLONASS-M satellite block.
    #[wasm_bindgen(js_name = GLONASS_M)]
    pub fn glonass_m() -> Self {
        Self(gnss::GnssBlock::GlonassM)
    }
    /// Galileo IOV satellite block.
    #[wasm_bindgen(js_name = GALILEO_IOV)]
    pub fn galileo_iov() -> Self {
        Self(gnss::GnssBlock::GalileoIov)
    }
    /// Galileo FOC satellite block.
    #[wasm_bindgen(js_name = GALILEO_FOC)]
    pub fn galileo_foc() -> Self {
        Self(gnss::GnssBlock::GalileoFoc)
    }
    /// BeiDou-2 GEO satellite block.
    #[wasm_bindgen(js_name = BEIDOU2_GEO)]
    pub fn beidou2_geo() -> Self {
        Self(gnss::GnssBlock::Beidou2Geo)
    }
    /// BeiDou-2 IGSO satellite block.
    #[wasm_bindgen(js_name = BEIDOU2_IGSO)]
    pub fn beidou2_igso() -> Self {
        Self(gnss::GnssBlock::Beidou2Igso)
    }
    /// BeiDou-2 MEO satellite block.
    #[wasm_bindgen(js_name = BEIDOU2_MEO)]
    pub fn beidou2_meo() -> Self {
        Self(gnss::GnssBlock::Beidou2Meo)
    }
    /// BeiDou-3 GEO satellite block.
    #[wasm_bindgen(js_name = BEIDOU3_GEO)]
    pub fn beidou3_geo() -> Self {
        Self(gnss::GnssBlock::Beidou3Geo)
    }
    /// BeiDou-3 IGSO satellite block.
    #[wasm_bindgen(js_name = BEIDOU3_IGSO)]
    pub fn beidou3_igso() -> Self {
        Self(gnss::GnssBlock::Beidou3Igso)
    }
    /// BeiDou-3 Meo block with an optional manufacturer.
    #[wasm_bindgen(js_name = beidou3Meo)]
    pub fn beidou3_meo(
        #[wasm_bindgen(unchecked_optional_param_type = "BeidouManufacturer | null")]
        manufacturer: Option<f64>,
    ) -> JsResult<Self> {
        Ok(Self(gnss::GnssBlock::Beidou3Meo {
            manufacturer: manufacturer.map(parse_manufacturer).transpose()?,
        }))
    }

    /// Parse an IGS or ANTEX satellite block name.
    #[wasm_bindgen(js_name = fromIgsName)]
    pub fn from_igs_name(name: &str) -> JsResult<Self> {
        gnss::GnssBlock::from_igs_name(name)
            .map(Self)
            .map_err(block_error)
    }

    /// Parse a constellation block type and optional BeiDou orbit class.
    #[wasm_bindgen(js_name = fromConstellationBlockType)]
    pub fn from_constellation_block_type(
        #[wasm_bindgen(unchecked_param_type = "GnssSystem")] system: f64,
        block_type: &str,
        #[wasm_bindgen(unchecked_optional_param_type = "BeidouOrbitClass | null")]
        beidou_orbit: Option<f64>,
    ) -> JsResult<Self> {
        gnss::GnssBlock::from_constellation_block_type(
            parse_system(system)?.into(),
            block_type,
            beidou_orbit.map(parse_beidou_orbit).transpose()?,
        )
        .map(Self)
        .map_err(block_error)
    }

    /// Canonical IGS name for the block.
    #[wasm_bindgen(js_name = igsName)]
    pub fn igs_name(&self) -> String {
        self.0.igs_name().to_owned()
    }

    /// Constellation of the block.
    pub fn system(&self) -> GnssSystem {
        self.0.system().into()
    }

    /// Compare the blocks in their canonical order.
    pub fn compare(&self, other: &GnssBlock) -> i32 {
        self.0.cmp(&other.0) as i32
    }

    /// Canonical IGS display name.
    #[wasm_bindgen(js_name = toString)]
    pub fn display_text(&self) -> String {
        self.0.to_string()
    }
}

/// Parameters shared by the Galileo FOC and BeiDou-3 Cast cosine yaw laws.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct CosineTurnParameters(gnss::CosineTurnParameters);

#[wasm_bindgen]
impl CosineTurnParameters {
    /// Compare the stored value.
    pub fn equals(&self, other: &CosineTurnParameters) -> bool {
        self.0 == other.0
    }

    /// Construct cosine turn parameters.
    #[wasm_bindgen(constructor)]
    pub fn new(
        beta_limit_rad: f64,
        beta_limit_inclusive: bool,
        #[wasm_bindgen(unchecked_param_type = "CosineTurnStartAngle")] start_angle: f64,
        start_angle_rad: f64,
        period_s: f64,
    ) -> JsResult<Self> {
        Ok(Self(gnss::CosineTurnParameters {
            beta_limit_rad,
            beta_limit_inclusive,
            start_angle: parse_start_angle(start_angle)?.into(),
            start_angle_rad,
            period_s,
        }))
    }

    /// Sun elevation limit in radians.
    #[wasm_bindgen(getter, js_name = betaLimitRad)]
    pub fn beta_limit_rad(&self) -> f64 {
        self.0.beta_limit_rad
    }

    /// Whether the elevation limit is inclusive.
    #[wasm_bindgen(getter, js_name = betaLimitInclusive)]
    pub fn beta_limit_inclusive(&self) -> bool {
        self.0.beta_limit_inclusive
    }

    /// Type of angle that begins the turn.
    #[wasm_bindgen(getter, js_name = startAngle)]
    pub fn start_angle(&self) -> JsResult<CosineTurnStartAngle> {
        self.0.start_angle.try_into()
    }

    /// Angle threshold in radians.
    #[wasm_bindgen(getter, js_name = startAngleRad)]
    pub fn start_angle_rad(&self) -> f64 {
        self.0.start_angle_rad
    }

    /// Cosine period in seconds, twice the turn duration.
    #[wasm_bindgen(getter, js_name = periodS)]
    pub fn period_s(&self) -> f64 {
        self.0.period_s
    }
}

/// Parameterized GNSS satellite yaw law.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct GnssAttitudeLaw(gnss::GnssAttitudeLaw);

#[wasm_bindgen]
impl GnssAttitudeLaw {
    /// Compare the stored value.
    pub fn equals(&self, other: &GnssAttitudeLaw) -> bool {
        self.0 == other.0
    }

    /// Construct the GPS IIA yaw law.
    #[wasm_bindgen(js_name = gpsIia)]
    pub fn gps_iia(yaw_rate_rad_s: f64, yaw_bias_rad: f64, shadow_limit_rad: f64) -> Self {
        Self(gnss::GnssAttitudeLaw::GpsIIA {
            yaw_rate_rad_s,
            yaw_bias_rad,
            shadow_limit_rad,
        })
    }

    /// Construct the GPS IIR yaw law.
    #[wasm_bindgen(js_name = gpsIir)]
    pub fn gps_iir(yaw_rate_rad_s: f64) -> Self {
        Self(gnss::GnssAttitudeLaw::GpsIIR { yaw_rate_rad_s })
    }

    /// Construct the GPS IIF yaw law.
    #[wasm_bindgen(js_name = gpsIif)]
    pub fn gps_iif(yaw_rate_rad_s: f64, yaw_bias_rad: f64, shadow_limit_rad: f64) -> Self {
        Self(gnss::GnssAttitudeLaw::GpsIIF {
            yaw_rate_rad_s,
            yaw_bias_rad,
            shadow_limit_rad,
        })
    }

    /// Construct the GLONASS-M yaw law.
    #[wasm_bindgen(js_name = glonassM)]
    pub fn glonass_m(yaw_rate_rad_s: f64, shadow_limit_rad: f64) -> Self {
        Self(gnss::GnssAttitudeLaw::GlonassM {
            yaw_rate_rad_s,
            shadow_limit_rad,
        })
    }

    /// Construct the Galileo IOV yaw law.
    #[wasm_bindgen(js_name = galileoIov)]
    pub fn galileo_iov(beta_x_rad: f64, beta_y_rad: f64) -> Self {
        Self(gnss::GnssAttitudeLaw::GalileoIov {
            beta_x_rad,
            beta_y_rad,
        })
    }

    /// Construct the cosine turn yaw law.
    #[wasm_bindgen(js_name = cosineTurn)]
    pub fn cosine_turn(parameters: &CosineTurnParameters) -> Self {
        Self(gnss::GnssAttitudeLaw::CosineTurn(parameters.0))
    }

    /// Construct the BeiDou orbit-normal switching law.
    #[wasm_bindgen(js_name = beidouOrbitNormalSwitch)]
    pub fn beidou_orbit_normal_switch(ys_to_on_beta_rad: f64, on_to_ys_beta_rad: f64) -> Self {
        Self(gnss::GnssAttitudeLaw::BeidouOrbitNormalSwitch {
            ys_to_on_beta_rad,
            on_to_ys_beta_rad,
        })
    }

    /// Construct the fixed beta yaw steering yaw law.
    #[wasm_bindgen(js_name = fixedBetaYawSteering)]
    pub fn fixed_beta_yaw_steering(beta0_rad: f64) -> Self {
        Self(gnss::GnssAttitudeLaw::FixedBetaYawSteering { beta0_rad })
    }

    /// Construct the BeiDou-3 MEO law for an unspecified manufacturer.
    #[wasm_bindgen(js_name = beidou3MeoUnknownManufacturer)]
    pub fn beidou3_meo_unknown_manufacturer(
        cast: &CosineTurnParameters,
        secm_beta0_rad: f64,
    ) -> Self {
        Self(gnss::GnssAttitudeLaw::Beidou3MeoUnknownManufacturer {
            cast: cast.0,
            secm_beta0_rad,
        })
    }

    /// Construct the orbit normal yaw law.
    #[wasm_bindgen(js_name = orbitNormal)]
    pub fn orbit_normal() -> Self {
        Self(gnss::GnssAttitudeLaw::OrbitNormal)
    }

    /// Use the published law and its parameters for a satellite block.
    #[wasm_bindgen(js_name = forBlock)]
    pub fn for_block(block: &GnssBlock, prn: Option<f64>) -> JsResult<Self> {
        gnss::GnssAttitudeLaw::for_block(block.0, prn.map(parse_prn).transpose()?)
            .map(Self)
            .map_err(attitude_error)
    }
}

/// Inertial satellite state and Sun position at one epoch.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct AttitudeSample(gnss::AttitudeSample);

#[wasm_bindgen]
impl AttitudeSample {
    /// Compare the stored value.
    pub fn equals(&self, other: &AttitudeSample) -> bool {
        self.0 == other.0
    }

    /// Build an attitude sample from inertial position, velocity, and Sun vectors.
    #[wasm_bindgen(constructor)]
    pub fn new(position_m: &[f64], velocity_m_s: &[f64], sun_position_m: &[f64]) -> JsResult<Self> {
        Ok(Self(gnss::AttitudeSample {
            position_m: vec3("positionM", position_m)?,
            velocity_m_s: vec3("velocityMS", velocity_m_s)?,
            sun_position_m: vec3("sunPositionM", sun_position_m)?,
        }))
    }

    /// Satellite position in metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.0.position_m.to_vec()
    }

    /// Satellite velocity in metres per second.
    #[wasm_bindgen(getter, js_name = velocityMS)]
    pub fn velocity_m_s(&self) -> Vec<f64> {
        self.0.velocity_m_s.to_vec()
    }

    /// Sun position in metres.
    #[wasm_bindgen(getter, js_name = sunPositionM)]
    pub fn sun_position_m(&self) -> Vec<f64> {
        self.0.sun_position_m.to_vec()
    }
}

/// Sun-fixed D, Y and B unit axes.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct DybAxes(pub(crate) gnss::DybAxes);

#[wasm_bindgen]
impl DybAxes {
    /// Compare the stored value.
    pub fn equals(&self, other: &DybAxes) -> bool {
        self.0 == other.0
    }

    /// Unit vector from satellite to Sun.
    #[wasm_bindgen(getter)]
    pub fn d(&self) -> Vec<f64> {
        self.0.d.to_vec()
    }

    /// Nominal solar panel axis.
    #[wasm_bindgen(getter)]
    pub fn y(&self) -> Vec<f64> {
        self.0.y.to_vec()
    }

    /// Cross product of D and Y.
    #[wasm_bindgen(getter)]
    pub fn b(&self) -> Vec<f64> {
        self.0.b.to_vec()
    }
}

/// Nominal satellite yaw and body axes.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct NominalYaw(gnss::NominalYaw);

#[wasm_bindgen]
impl NominalYaw {
    /// Compare the stored value.
    pub fn equals(&self, other: &NominalYaw) -> bool {
        self.0 == other.0
    }

    /// Yaw in radians.
    #[wasm_bindgen(getter, js_name = yawRad)]
    pub fn yaw_rad(&self) -> f64 {
        self.0.yaw_rad
    }

    /// Sun elevation in radians.
    #[wasm_bindgen(getter, js_name = betaRad)]
    pub fn beta_rad(&self) -> f64 {
        self.0.beta_rad
    }

    /// Unit positive X body axis.
    #[wasm_bindgen(getter, js_name = bodyX)]
    pub fn body_x(&self) -> Vec<f64> {
        self.0.body_x.to_vec()
    }

    /// Unit positive Y body axis.
    #[wasm_bindgen(getter, js_name = bodyY)]
    pub fn body_y(&self) -> Vec<f64> {
        self.0.body_y.to_vec()
    }

    /// Unit positive Z body axis.
    #[wasm_bindgen(getter, js_name = bodyZ)]
    pub fn body_z(&self) -> Vec<f64> {
        self.0.body_z.to_vec()
    }
}

/// GNSS satellite attitude and Sun geometry.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct GnssAttitude(gnss::GnssAttitude);

#[wasm_bindgen]
impl GnssAttitude {
    /// Compare the stored value.
    pub fn equals(&self, other: &GnssAttitude) -> bool {
        self.0 == other.0
    }

    /// Active mode of the yaw law.
    #[wasm_bindgen(getter)]
    pub fn mode(&self) -> JsResult<AttitudeMode> {
        self.0.mode.try_into()
    }

    /// Yaw in radians.
    #[wasm_bindgen(getter, js_name = yawRad)]
    pub fn yaw_rad(&self) -> f64 {
        self.0.yaw_rad
    }

    /// Nominal yaw in radians.
    #[wasm_bindgen(getter, js_name = nominalYawRad)]
    pub fn nominal_yaw_rad(&self) -> f64 {
        self.0.nominal_yaw_rad
    }

    /// Sun elevation in radians.
    #[wasm_bindgen(getter, js_name = betaRad)]
    pub fn beta_rad(&self) -> f64 {
        self.0.beta_rad
    }

    /// Orbit angle from midnight in radians.
    #[wasm_bindgen(getter, js_name = orbitAngleRad)]
    pub fn orbit_angle_rad(&self) -> f64 {
        self.0.orbit_angle_rad
    }

    /// Unit positive X body axis.
    #[wasm_bindgen(getter, js_name = bodyX)]
    pub fn body_x(&self) -> Vec<f64> {
        self.0.body_x.to_vec()
    }

    /// Unit positive Y body axis.
    #[wasm_bindgen(getter, js_name = bodyY)]
    pub fn body_y(&self) -> Vec<f64> {
        self.0.body_y.to_vec()
    }

    /// Unit positive Z body axis.
    #[wasm_bindgen(getter, js_name = bodyZ)]
    pub fn body_z(&self) -> Vec<f64> {
        self.0.body_z.to_vec()
    }

    /// Sun-fixed axes if defined.
    #[wasm_bindgen(getter)]
    pub fn dyb(&self) -> Option<DybAxes> {
        self.0.dyb.map(DybAxes)
    }
}

#[derive(Clone)]
enum EarthFixedOrbit {
    Sp3(Arc<ephemeris::Sp3>),
    Broadcast(Arc<ephemeris::BroadcastEphemeris>),
    Scoring(OrbitSource),
}

#[derive(Clone)]
enum AttitudeOrbit {
    EarthFixed {
        orbit: EarthFixedOrbit,
        satellite: GnssSatelliteId,
        orientation: Arc<dyn EarthOrientationProvider>,
    },
    EcksteinHechler(Arc<sidereon_core::astro::propagator::analytic::EcksteinHechlerPropagator>),
    BrouwerLyddane(Arc<sidereon_core::astro::propagator::analytic::BrouwerLyddanePropagator>),
    Dense(DenseOutput),
}

/// A satellite orbit and Sun ephemeris for GNSS attitude evaluation.
/// Epochs are TDB seconds past J2000. Samples contain GCRF vectors.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OrbitAttitudeTrajectory {
    orbit: AttitudeOrbit,
    de: Option<Arc<CoreDeEphemeris>>,
    half_step_s: f64,
}

impl OrbitAttitudeTrajectory {
    fn new(orbit: AttitudeOrbit) -> Self {
        Self {
            orbit,
            de: None,
            half_step_s: DEFAULT_VELOCITY_HALF_STEP_S,
        }
    }

    fn earth_fixed(
        orbit: EarthFixedOrbit,
        satellite: &str,
        orientation: Option<&Iers2010EarthOrientationProvider>,
    ) -> JsResult<Self> {
        let satellite = parse_sat(satellite)?;
        let orientation: Arc<dyn EarthOrientationProvider> = match orientation {
            Some(provider) => provider.0.clone(),
            None => Arc::new(TdbEarthOrientationProvider::new()),
        };
        Ok(Self::new(AttitudeOrbit::EarthFixed {
            orbit,
            satellite,
            orientation,
        }))
    }

    fn evaluate<T>(
        &self,
        operation: impl FnOnce(gnss::OrbitAttitudeTrajectory<'_>) -> T,
    ) -> Result<T, OrbitScoreError> {
        use gnss::OrbitAttitudeTrajectory as CoreTrajectory;

        let evaluate = |trajectory: CoreTrajectory<'_>| {
            let mut trajectory = trajectory.with_velocity_half_step_s(self.half_step_s)?;
            if let Some(ephemeris) = self.de.as_deref() {
                trajectory = trajectory.with_de_ephemeris(ephemeris);
            }
            Ok(operation(trajectory))
        };
        match &self.orbit {
            AttitudeOrbit::EarthFixed {
                orbit,
                satellite,
                orientation,
            } => {
                match orbit {
                    EarthFixedOrbit::Sp3(product) => evaluate(CoreTrajectory::from_sp3(
                        product,
                        *satellite,
                        orientation.as_ref(),
                    )),
                    EarthFixedOrbit::Broadcast(ephemeris) => evaluate(
                        CoreTrajectory::from_broadcast(ephemeris, *satellite, orientation.as_ref()),
                    ),
                    EarthFixedOrbit::Scoring(source) => source.with_source(|source| {
                        evaluate(CoreTrajectory::from_orbit_source(
                            source,
                            *satellite,
                            orientation.as_ref(),
                        ))
                    }),
                }
            }
            AttitudeOrbit::EcksteinHechler(propagator) => {
                evaluate(CoreTrajectory::from_eckstein_hechler(propagator))
            }
            AttitudeOrbit::BrouwerLyddane(propagator) => {
                evaluate(CoreTrajectory::from_brouwer_lyddane(propagator))
            }
            AttitudeOrbit::Dense(output) => evaluate(CoreTrajectory::from_dense_output(&output.0)),
        }
    }
}

#[wasm_bindgen]
impl OrbitAttitudeTrajectory {
    /// Use a parsed SP3 product aligned with the provider's ITRF.
    /// Omit the provider for default TDB orientation.
    #[wasm_bindgen(js_name = fromSp3)]
    pub fn from_sp3(
        product: &Sp3,
        satellite: &str,
        orientation: Option<Iers2010EarthOrientationProvider>,
    ) -> JsResult<Self> {
        Self::earth_fixed(
            EarthFixedOrbit::Sp3(Arc::new(product.inner.clone())),
            satellite,
            orientation.as_ref(),
        )
    }

    /// Use a broadcast navigation set aligned with the provider's ITRF.
    /// Omit the provider for default TDB orientation.
    #[wasm_bindgen(js_name = fromBroadcast)]
    pub fn from_broadcast(
        ephemeris: &BroadcastEphemeris,
        satellite: &str,
        orientation: Option<Iers2010EarthOrientationProvider>,
    ) -> JsResult<Self> {
        Self::earth_fixed(
            EarthFixedOrbit::Broadcast(ephemeris.inner.clone()),
            satellite,
            orientation.as_ref(),
        )
    }

    /// Use a PredictedSp3, BroadcastOrbit, OrbitTrajectory or shared OrbitSource handle.
    /// Omit the provider for default TDB orientation.
    #[wasm_bindgen(js_name = fromOrbitSource)]
    pub fn from_orbit_source(
        source: &OrbitSource,
        satellite: &str,
        orientation: Option<Iers2010EarthOrientationProvider>,
    ) -> JsResult<Self> {
        Self::earth_fixed(
            EarthFixedOrbit::Scoring(source.clone()),
            satellite,
            orientation.as_ref(),
        )
    }

    /// Use an Eckstein–Hechler propagator in GCRF.
    #[wasm_bindgen(js_name = fromEcksteinHechler)]
    pub fn from_eckstein_hechler(propagator: &EcksteinHechlerPropagator) -> Self {
        Self::new(AttitudeOrbit::EcksteinHechler(Arc::new(
            propagator.0.clone(),
        )))
    }

    /// Use a Brouwer–Lyddane propagator in GCRF.
    #[wasm_bindgen(js_name = fromBrouwerLyddane)]
    pub fn from_brouwer_lyddane(propagator: &BrouwerLyddanePropagator) -> Self {
        Self::new(AttitudeOrbit::BrouwerLyddane(Arc::new(
            propagator.0.clone(),
        )))
    }

    /// Use continuous DP5(4) output in GCRF.
    #[wasm_bindgen(js_name = fromDenseOutput)]
    pub fn from_dense_output(output: &DenseOutput) -> Self {
        Self::new(AttitudeOrbit::Dense(output.clone()))
    }

    /// Return a trajectory with JPL DE Sun positions.
    #[wasm_bindgen(js_name = withDeEphemeris)]
    pub fn with_de_ephemeris(&self, ephemeris: &DeEphemeris) -> Self {
        Self {
            de: Some(ephemeris.0.clone()),
            ..self.clone()
        }
    }

    /// Return a trajectory with a different velocity difference half step for Earth-fixed orbits.
    /// Throws OrbitScoreError if the step is non-finite or below MIN_VELOCITY_HALF_STEP_S.
    #[wasm_bindgen(js_name = withVelocityHalfStepS)]
    pub fn with_velocity_half_step_s(&self, half_step_s: f64) -> JsResult<Self> {
        let trajectory = Self {
            half_step_s,
            ..self.clone()
        };
        trajectory.evaluate(|_| ()).map_err(score_error)?;
        Ok(trajectory)
    }

    /// Sample at TDB seconds past J2000. The GCRF vectors are in metres and metres per second.
    /// Returns undefined when the orbit, orientation or Sun ephemeris cannot supply the epoch.
    pub fn sample(&self, t_s: f64) -> JsResult<Option<AttitudeSample>> {
        self.evaluate(|trajectory| trajectory.sample(t_s).map(AttitudeSample))
            .map_err(score_error)
    }
}

/// Evaluate a GNSS attitude law at TDB seconds past J2000, returning GCRF axes.
#[wasm_bindgen(js_name = gnssAttitude)]
pub fn gnss_attitude(
    law: &GnssAttitudeLaw,
    trajectory: &OrbitAttitudeTrajectory,
    t_s: f64,
) -> JsResult<GnssAttitude> {
    trajectory
        .evaluate(|trajectory| gnss::gnss_attitude(&law.0, &trajectory, t_s))
        .map_err(score_error)?
        .map(GnssAttitude)
        .map_err(attitude_error)
}

/// Nominal yaw steering from one inertial satellite state and Sun position.
#[wasm_bindgen(js_name = nominalYaw)]
pub fn nominal_yaw(
    position_m: &[f64],
    velocity_m_s: &[f64],
    sun_position_m: &[f64],
) -> JsResult<NominalYaw> {
    gnss::nominal_yaw(
        vec3("positionM", position_m)?,
        vec3("velocityMS", velocity_m_s)?,
        vec3("sunPositionM", sun_position_m)?,
    )
    .map(NominalYaw)
    .map_err(attitude_error)
}

/// Sun-fixed axes from inertial satellite and Sun positions.
#[wasm_bindgen(js_name = dybAxes)]
pub fn dyb_axes(position_m: &[f64], sun_position_m: &[f64]) -> JsResult<DybAxes> {
    gnss::dyb_axes(
        vec3("positionM", position_m)?,
        vec3("sunPositionM", sun_position_m)?,
    )
    .map(DybAxes)
    .map_err(attitude_error)
}

/// Published GPS IIA maximum yaw rate for a PRN in radians per second.
#[wasm_bindgen(js_name = gpsIiaDefaultYawRateRadS)]
pub fn gps_iia_default_yaw_rate_rad_s(prn: f64) -> JsResult<Option<f64>> {
    Ok(gnss::gps_iia_default_yaw_rate_rad_s(parse_prn(prn)?))
}

/// Published gps iia default yaw bias rad.
#[wasm_bindgen(js_name = GPS_IIA_DEFAULT_YAW_BIAS_RAD)]
pub fn gps_iia_default_yaw_bias_rad() -> f64 {
    gnss::GPS_IIA_DEFAULT_YAW_BIAS_RAD
}

/// Published gps iir default yaw rate rad s.
#[wasm_bindgen(js_name = GPS_IIR_DEFAULT_YAW_RATE_RAD_S)]
pub fn gps_iir_default_yaw_rate_rad_s() -> f64 {
    gnss::GPS_IIR_DEFAULT_YAW_RATE_RAD_S
}

/// Published gps iif default yaw rate rad s.
#[wasm_bindgen(js_name = GPS_IIF_DEFAULT_YAW_RATE_RAD_S)]
pub fn gps_iif_default_yaw_rate_rad_s() -> f64 {
    gnss::GPS_IIF_DEFAULT_YAW_RATE_RAD_S
}

/// Published gps iif default yaw bias rad.
#[wasm_bindgen(js_name = GPS_IIF_DEFAULT_YAW_BIAS_RAD)]
pub fn gps_iif_default_yaw_bias_rad() -> f64 {
    gnss::GPS_IIF_DEFAULT_YAW_BIAS_RAD
}

/// Published gps shadow limit rad.
#[wasm_bindgen(js_name = GPS_SHADOW_LIMIT_RAD)]
pub fn gps_shadow_limit_rad() -> f64 {
    gnss::GPS_SHADOW_LIMIT_RAD
}

/// Published glonass m default yaw rate rad s.
#[wasm_bindgen(js_name = GLONASS_M_DEFAULT_YAW_RATE_RAD_S)]
pub fn glonass_m_default_yaw_rate_rad_s() -> f64 {
    gnss::GLONASS_M_DEFAULT_YAW_RATE_RAD_S
}

/// Published glonass m shadow limit rad.
#[wasm_bindgen(js_name = GLONASS_M_SHADOW_LIMIT_RAD)]
pub fn glonass_m_shadow_limit_rad() -> f64 {
    gnss::GLONASS_M_SHADOW_LIMIT_RAD
}

/// Published galileo iov beta x rad.
#[wasm_bindgen(js_name = GALILEO_IOV_BETA_X_RAD)]
pub fn galileo_iov_beta_x_rad() -> f64 {
    gnss::GALILEO_IOV_BETA_X_RAD
}

/// Published galileo iov beta y rad.
#[wasm_bindgen(js_name = GALILEO_IOV_BETA_Y_RAD)]
pub fn galileo_iov_beta_y_rad() -> f64 {
    gnss::GALILEO_IOV_BETA_Y_RAD
}

/// Published gps iii beta x rad.
#[wasm_bindgen(js_name = GPS_III_BETA_X_RAD)]
pub fn gps_iii_beta_x_rad() -> f64 {
    gnss::GPS_III_BETA_X_RAD
}

/// Published gps iii beta y rad.
#[wasm_bindgen(js_name = GPS_III_BETA_Y_RAD)]
pub fn gps_iii_beta_y_rad() -> f64 {
    gnss::GPS_III_BETA_Y_RAD
}

/// Published galileo foc turn.
#[wasm_bindgen(js_name = GALILEO_FOC_TURN)]
pub fn galileo_foc_turn() -> CosineTurnParameters {
    CosineTurnParameters(gnss::GALILEO_FOC_TURN)
}

/// Published beidou3 cast meo turn.
#[wasm_bindgen(js_name = BEIDOU3_CAST_MEO_TURN)]
pub fn beidou3_cast_meo_turn() -> CosineTurnParameters {
    CosineTurnParameters(gnss::BEIDOU3_CAST_MEO_TURN)
}

/// Published beidou3 cast igso turn.
#[wasm_bindgen(js_name = BEIDOU3_CAST_IGSO_TURN)]
pub fn beidou3_cast_igso_turn() -> CosineTurnParameters {
    CosineTurnParameters(gnss::BEIDOU3_CAST_IGSO_TURN)
}

/// Published beidou3 secm beta0 rad.
#[wasm_bindgen(js_name = BEIDOU3_SECM_BETA0_RAD)]
pub fn beidou3_secm_beta0_rad() -> f64 {
    gnss::BEIDOU3_SECM_BETA0_RAD
}

/// Published beidou2 ys to on beta rad.
#[wasm_bindgen(js_name = BEIDOU2_YS_TO_ON_BETA_RAD)]
pub fn beidou2_ys_to_on_beta_rad() -> f64 {
    gnss::BEIDOU2_YS_TO_ON_BETA_RAD
}

/// Published beidou2 on to ys beta rad.
#[wasm_bindgen(js_name = BEIDOU2_ON_TO_YS_BETA_RAD)]
pub fn beidou2_on_to_ys_beta_rad() -> f64 {
    gnss::BEIDOU2_ON_TO_YS_BETA_RAD
}

#[wasm_bindgen(typescript_custom_section)]
const ATTITUDE_TYPES: &str = r#"
/** Details returned by GnssAttitudeError. */
export type GnssAttitudeErrorDetail =
    | { kind: "nonFinite"; field: string }
    | { kind: "zeroLength"; field: string }
    | { kind: "trajectoryUnavailable"; tS: number }
    | { kind: "invalidParameter"; field: string; value: number }
    | { kind: "noDefaultYawRate"; prn: number | null }
    | { kind: "degenerateGeometry"; tS: number | null }
    | { kind: "beidouManufacturerRequired"; tS: number }
    | { kind: "turnEndNotFound"; tS: number }
    | { kind: "outsideValidatedDomain"; tS: number; reason: string }
    | { kind: "unknown" };

/** GnssAttitudeError with the core diagnostic message and structured detail. */
export interface GnssAttitudeError extends Error {
    readonly name: "GnssAttitudeError";
    readonly kind: GnssAttitudeErrorDetail["kind"];
    readonly detail: GnssAttitudeErrorDetail;
}

/** Details returned by GnssBlockError. */
export type GnssBlockErrorDetail =
    | { kind: "unknown"; name: string }
    | { kind: "unsupported"; name: string }
    | { kind: "beidouOrbitClassRequired"; blockType: string }
    | { kind: "unknownVariant" };

/** GnssBlockError with the core diagnostic message and structured detail. */
export interface GnssBlockError extends Error {
    readonly name: "GnssBlockError";
    readonly kind: GnssBlockErrorDetail["kind"];
    readonly detail: GnssBlockErrorDetail;
}
"#;

/// Compare two BeidouManufacturer values in the core order.
#[wasm_bindgen(js_name = beidouManufacturerCompare)]
pub fn beidou_manufacturer_compare(
    #[wasm_bindgen(unchecked_param_type = "BeidouManufacturer")] left: f64,
    #[wasm_bindgen(unchecked_param_type = "BeidouManufacturer")] right: f64,
) -> Result<i32, JsValue> {
    Ok(parse_manufacturer(left)?.cmp(&parse_manufacturer(right)?) as i32)
}

/// Compare two BeidouOrbitClass values in the core order.
#[wasm_bindgen(js_name = beidouOrbitClassCompare)]
pub fn beidou_orbit_class_compare(
    #[wasm_bindgen(unchecked_param_type = "BeidouOrbitClass")] left: f64,
    #[wasm_bindgen(unchecked_param_type = "BeidouOrbitClass")] right: f64,
) -> Result<i32, JsValue> {
    Ok(parse_beidou_orbit(left)?.cmp(&parse_beidou_orbit(right)?) as i32)
}

#[cfg(test)]
mod tests {
    use super::{AttitudeMode, GnssAttitude};
    use sidereon_core::astro::attitude::gnss;

    #[test]
    fn attitude_result_preserves_axes_and_angles() {
        let geometry = gnss::GnssAttitude {
            mode: gnss::AttitudeMode::OrbitNormal,
            yaw_rad: 0.0,
            nominal_yaw_rad: 0.2,
            beta_rad: 0.3,
            orbit_angle_rad: 0.4,
            body_x: [0.0, 1.0, 0.0],
            body_y: [0.0, 0.0, -1.0],
            body_z: [-1.0, 0.0, 0.0],
            dyb: Some(gnss::DybAxes {
                d: [0.0, 1.0, 0.0],
                y: [0.0, 0.0, -1.0],
                b: [-1.0, 0.0, 0.0],
            }),
        };
        let attitude = GnssAttitude(geometry);
        assert!(attitude.mode().unwrap() == AttitudeMode::OrbitNormal);
        assert_eq!(attitude.yaw_rad(), 0.0);
        assert_eq!(attitude.nominal_yaw_rad(), 0.2);
        assert_eq!(attitude.beta_rad(), 0.3);
        assert_eq!(attitude.orbit_angle_rad(), 0.4);
        assert_eq!(attitude.body_x(), [0.0, 1.0, 0.0]);
        assert_eq!(attitude.body_y(), [0.0, 0.0, -1.0]);
        assert_eq!(attitude.body_z(), [-1.0, 0.0, 0.0]);
        assert_eq!(attitude.dyb().unwrap().b(), [-1.0, 0.0, 0.0]);
        assert!(attitude.equals(&GnssAttitude(geometry)));
        assert!(!attitude.equals(&GnssAttitude(gnss::GnssAttitude {
            dyb: None,
            ..geometry
        })));
    }
}
