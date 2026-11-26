//! Geocentric GCRF states from JPL DE binary ephemerides.

use std::sync::Arc;

use serde_json::json;
use sidereon_core::astro::de::{self, DeError};
use wasm_bindgen::prelude::*;

use crate::bodies::SunMoon;
use crate::error::{range_error, typed_error};

fn de_error(error: DeError) -> JsValue {
    let detail = match &error {
        DeError::Io(source) => {
            json!({"kind":"io","message":source.to_string(),"rawOsError":source.raw_os_error()})
        }
        DeError::InvalidFile(reason) => json!({"kind":"invalidFile","reason":reason}),
        DeError::NonFiniteEpoch => json!({"kind":"nonFiniteEpoch"}),
        DeError::OutsideCoverage { epoch_tdb_seconds } => {
            json!({"kind":"outsideCoverage","epochTdbSeconds":epoch_tdb_seconds})
        }
        DeError::NonFiniteState => json!({"kind":"nonFiniteState"}),
        _ => json!({"kind":"unknown"}),
    };
    typed_error("DeError", &error.to_string(), detail)
}

/// Solar-system body whose gravitational parameter can appear in a DE header.
#[wasm_bindgen]
pub enum DeBody {
    /// Sun.
    Sun,
    /// Mercury.
    Mercury,
    /// Venus.
    Venus,
    /// Earth–Moon barycentre.
    EarthMoon,
    /// Earth.
    Earth,
    /// Moon.
    Moon,
    /// Mars system.
    Mars,
    /// Jupiter system.
    Jupiter,
    /// Saturn system.
    Saturn,
    /// Uranus system.
    Uranus,
    /// Neptune system.
    Neptune,
    /// Pluto system.
    Pluto,
    /// Entire solar system.
    SolarSystem,
}

fn de_body(value: f64) -> Result<de::DeBody, JsValue> {
    use de::DeBody as B;
    const BODIES: [B; 13] = [
        B::Sun,
        B::Mercury,
        B::Venus,
        B::EarthMoon,
        B::Earth,
        B::Moon,
        B::Mars,
        B::Jupiter,
        B::Saturn,
        B::Uranus,
        B::Neptune,
        B::Pluto,
        B::SolarSystem,
    ];
    let index = crate::error::index_arg(value, "body")?;
    BODIES
        .get(index)
        .copied()
        .ok_or_else(|| range_error("body must be a DeBody value"))
}

/// Geocentric Cartesian position and velocity in GCRF.
#[wasm_bindgen]
pub struct DeState(de::DeState);

#[wasm_bindgen]
impl DeState {
    /// Position in metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.0.position_m.to_vec()
    }

    /// Velocity in metres per second.
    #[wasm_bindgen(getter, js_name = velocityMS)]
    pub fn velocity_m_s(&self) -> Vec<f64> {
        self.0.velocity_m_s.to_vec()
    }

    /// Compare both vectors by value.
    pub fn equals(&self, other: &DeState) -> bool {
        self.0 == other.0
    }
}

/// Geocentric Sun and Moon states at one epoch.
#[wasm_bindgen]
pub struct DeSunMoon(de::DeSunMoon);

#[wasm_bindgen]
impl DeSunMoon {
    /// Sun state.
    #[wasm_bindgen(getter)]
    pub fn sun(&self) -> DeState {
        DeState(self.0.sun)
    }

    /// Moon state.
    #[wasm_bindgen(getter)]
    pub fn moon(&self) -> DeState {
        DeState(self.0.moon)
    }

    /// Compare both states by value.
    pub fn equals(&self, other: &DeSunMoon) -> bool {
        self.0 == other.0
    }
}

/// Immutable JPL DE binary ephemeris.
#[wasm_bindgen]
pub struct DeEphemeris(pub(crate) Arc<de::DeEphemeris>);

#[wasm_bindgen]
impl DeEphemeris {
    /// Parse a DE binary from bytes.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(data: &[u8]) -> Result<DeEphemeris, JsValue> {
        de::DeEphemeris::from_bytes(data)
            .map(Arc::new)
            .map(DeEphemeris)
            .map_err(de_error)
    }

    /// DE ephemeris version number.
    pub fn version(&self) -> u32 {
        self.0.version()
    }

    /// Number of bytes in each binary record.
    #[wasm_bindgen(js_name = recordSize)]
    pub fn record_size(&self) -> usize {
        self.0.record_size()
    }

    /// Start and end epochs in TDB seconds from J2000.
    #[wasm_bindgen(js_name = coverageTdbSeconds)]
    pub fn coverage_tdb_seconds(&self) -> Vec<f64> {
        let (start, end) = self.0.coverage_tdb_seconds();
        vec![start, end]
    }

    /// Named constants in the binary header.
    #[wasm_bindgen(unchecked_return_type = "Map<string, number>")]
    pub fn constants(&self) -> JsValue {
        let constants = js_sys::Map::new();
        for (name, value) in self.0.constants() {
            constants.set(&JsValue::from_str(name), &JsValue::from_f64(*value));
        }
        constants.into()
    }

    /// Astronomical unit in metres.
    #[wasm_bindgen(js_name = astronomicalUnitM)]
    pub fn astronomical_unit_m(&self) -> f64 {
        self.0.astronomical_unit_m()
    }

    /// Earth-to-Moon mass ratio from the header.
    #[wasm_bindgen(js_name = earthMoonMassRatio)]
    pub fn earth_moon_mass_ratio(&self) -> f64 {
        self.0.earth_moon_mass_ratio()
    }

    /// Gravitational mass parameter in cubic metres per square second, if present.
    #[wasm_bindgen(js_name = gmM3S2)]
    pub fn gm_m3_s2(
        &self,
        #[wasm_bindgen(unchecked_param_type = "DeBody")] body: f64,
    ) -> Result<Option<f64>, JsValue> {
        Ok(self.0.gm_m3_s2(de_body(body)?))
    }

    /// Geocentric GCRF positions in metres at TDB seconds from J2000.
    #[wasm_bindgen(js_name = sunMoonPositionsAtTdbSeconds)]
    pub fn sun_moon_positions_at_tdb_seconds(
        &self,
        epoch_tdb_seconds: f64,
    ) -> Result<SunMoon, JsValue> {
        self.0
            .sun_moon_positions_at_tdb_seconds(epoch_tdb_seconds)
            .map(SunMoon::geocentric)
            .map_err(de_error)
    }

    /// Geocentric GCRF states at TDB seconds from J2000.
    #[wasm_bindgen(js_name = sunMoonAtTdbSeconds)]
    pub fn sun_moon_at_tdb_seconds(&self, epoch_tdb_seconds: f64) -> Result<DeSunMoon, JsValue> {
        self.0
            .sun_moon_at_tdb_seconds(epoch_tdb_seconds)
            .map(DeSunMoon)
            .map_err(de_error)
    }
}

#[wasm_bindgen(typescript_custom_section)]
const DE_ERROR_TYPE: &str = r#"
/** A JPL DE parse or evaluation failure. */
export type DeErrorDetail =
    | { kind: "invalidFile"; reason: string }
    | { kind: "nonFiniteEpoch" }
    | { kind: "outsideCoverage"; epochTdbSeconds: number }
    | { kind: "nonFiniteState" }
    | { kind: "unknown" };

export interface DeError extends Error {
    readonly name: "DeError";
    readonly kind: DeErrorDetail["kind"];
    readonly detail: DeErrorDetail;
}
"#;
