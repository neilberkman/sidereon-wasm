use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon::almanac::{
    lunar_solar_eclipses as core_lunar_solar_eclipses, meridian_transits as core_meridian_transits,
    moon_phases as core_moon_phases, planetary_events as core_planetary_events,
    seasons as core_seasons, CulminationKind, EclipseKind, EphemerisSource, MoonPhaseKind, Planet,
    PlanetaryEventKind, SeasonKind, TransitBody,
};
use sidereon::passes::UtcInstant;
use sidereon_core::astro::frames::transforms::GeodeticStationKm;

use crate::error::{engine_error, type_error};
use crate::spk::Spk;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationInput {
    latitude_deg: f64,
    longitude_deg: f64,
    #[serde(default)]
    altitude_km: f64,
}

impl StationInput {
    fn to_core(&self) -> GeodeticStationKm {
        GeodeticStationKm {
            latitude_deg: self.latitude_deg,
            longitude_deg: self.longitude_deg,
            altitude_km: self.altitude_km,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimedKindJs {
    time_unix_us: i64,
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanetaryEventJs {
    time_unix_us: i64,
    planet: &'static str,
    kind: &'static str,
    elongation_deg: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TransitEventJs {
    time_unix_us: i64,
    kind: &'static str,
    altitude_deg: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EclipseEventJs {
    time_maximum_unix_us: i64,
    kind: &'static str,
    magnitude: f64,
    moon_latitude_deg: f64,
    gamma: f64,
    uncertain: bool,
}

fn instant(us: i64) -> UtcInstant {
    UtcInstant::from_unix_microseconds(us)
}

fn season_label(kind: SeasonKind) -> &'static str {
    match kind {
        SeasonKind::MarchEquinox => "marchEquinox",
        SeasonKind::JuneSolstice => "juneSolstice",
        SeasonKind::SeptemberEquinox => "septemberEquinox",
        SeasonKind::DecemberSolstice => "decemberSolstice",
        _ => "unknown",
    }
}

fn phase_label(kind: MoonPhaseKind) -> &'static str {
    match kind {
        MoonPhaseKind::New => "new",
        MoonPhaseKind::FirstQuarter => "firstQuarter",
        MoonPhaseKind::Full => "full",
        MoonPhaseKind::LastQuarter => "lastQuarter",
        _ => "unknown",
    }
}

fn planet_label(planet: Planet) -> &'static str {
    match planet {
        Planet::Mercury => "mercury",
        Planet::Venus => "venus",
        Planet::Mars => "mars",
        Planet::Jupiter => "jupiter",
        Planet::Saturn => "saturn",
        Planet::Uranus => "uranus",
        Planet::Neptune => "neptune",
        _ => "unknown",
    }
}

fn parse_planet(value: &str) -> Result<Planet, JsValue> {
    match value {
        "mercury" => Ok(Planet::Mercury),
        "venus" => Ok(Planet::Venus),
        "mars" => Ok(Planet::Mars),
        "jupiter" => Ok(Planet::Jupiter),
        "saturn" => Ok(Planet::Saturn),
        "uranus" => Ok(Planet::Uranus),
        "neptune" => Ok(Planet::Neptune),
        other => Err(type_error(&format!("invalid planet {other:?}"))),
    }
}

fn parse_planet_event_kind(value: &str) -> Result<PlanetaryEventKind, JsValue> {
    match value {
        "conjunction" => Ok(PlanetaryEventKind::Conjunction),
        "opposition" => Ok(PlanetaryEventKind::Opposition),
        other => Err(type_error(&format!(
            "invalid planetary event kind {other:?}: expected \"conjunction\" or \"opposition\""
        ))),
    }
}

fn planetary_kind_label(kind: PlanetaryEventKind) -> &'static str {
    match kind {
        PlanetaryEventKind::Conjunction => "conjunction",
        PlanetaryEventKind::Opposition => "opposition",
        _ => "unknown",
    }
}

fn culmination_label(kind: CulminationKind) -> &'static str {
    match kind {
        CulminationKind::Upper => "upper",
        CulminationKind::Lower => "lower",
        _ => "unknown",
    }
}

fn eclipse_label(kind: EclipseKind) -> &'static str {
    match kind {
        EclipseKind::LunarPenumbral => "lunarPenumbral",
        EclipseKind::LunarPartial => "lunarPartial",
        EclipseKind::LunarTotal => "lunarTotal",
        EclipseKind::SolarPartial => "solarPartial",
        EclipseKind::SolarAnnular => "solarAnnular",
        EclipseKind::SolarTotal => "solarTotal",
        EclipseKind::SolarHybrid => "solarHybrid",
        _ => "unknown",
    }
}

fn source_spk(spk: &Spk) -> EphemerisSource<'_> {
    EphemerisSource::Spk(spk.core())
}

fn seasons_with_source(
    source: EphemerisSource<'_>,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    let events = core_seasons(
        source,
        instant(start_unix_us),
        instant(end_unix_us),
        step_s,
        tolerance_s,
    )
    .map_err(engine_error)?;
    let out: Vec<TimedKindJs> = events
        .into_iter()
        .map(|event| TimedKindJs {
            time_unix_us: event.time.unix_microseconds(),
            kind: season_label(event.kind),
        })
        .collect();
    serde_wasm_bindgen::to_value(&out).map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name = seasons)]
pub fn seasons(
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    seasons_with_source(
        EphemerisSource::Analytic,
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}

#[wasm_bindgen(js_name = seasonsSpk)]
pub fn seasons_spk(
    spk: &Spk,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    seasons_with_source(
        source_spk(spk),
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}

fn moon_phases_with_source(
    source: EphemerisSource<'_>,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    let events = core_moon_phases(
        source,
        instant(start_unix_us),
        instant(end_unix_us),
        step_s,
        tolerance_s,
    )
    .map_err(engine_error)?;
    let out: Vec<TimedKindJs> = events
        .into_iter()
        .map(|event| TimedKindJs {
            time_unix_us: event.time.unix_microseconds(),
            kind: phase_label(event.kind),
        })
        .collect();
    serde_wasm_bindgen::to_value(&out).map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name = moonPhases)]
pub fn moon_phases(
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    moon_phases_with_source(
        EphemerisSource::Analytic,
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}

#[wasm_bindgen(js_name = moonPhasesSpk)]
pub fn moon_phases_spk(
    spk: &Spk,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    moon_phases_with_source(
        source_spk(spk),
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}

#[wasm_bindgen(js_name = planetaryEvents)]
pub fn planetary_events(
    spk: &Spk,
    planet: &str,
    kind: &str,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    let planet = parse_planet(planet)?;
    let kind = parse_planet_event_kind(kind)?;
    let events = core_planetary_events(
        source_spk(spk),
        planet,
        kind,
        instant(start_unix_us),
        instant(end_unix_us),
        step_s,
        tolerance_s,
    )
    .map_err(engine_error)?;
    let out: Vec<PlanetaryEventJs> = events
        .into_iter()
        .map(|event| PlanetaryEventJs {
            time_unix_us: event.time.unix_microseconds(),
            planet: planet_label(event.planet),
            kind: planetary_kind_label(event.kind),
            elongation_deg: event.elongation_deg,
        })
        .collect();
    serde_wasm_bindgen::to_value(&out).map_err(|e| type_error(&e.to_string()))
}

fn parse_transit_body(value: &str) -> Result<TransitBody, JsValue> {
    match value {
        "sun" => Ok(TransitBody::Sun),
        "moon" => Ok(TransitBody::Moon),
        other => parse_planet(other).map(TransitBody::Planet),
    }
}

fn transits_with_source(
    source: EphemerisSource<'_>,
    body: &str,
    station: JsValue,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    let station: StationInput = serde_wasm_bindgen::from_value(station)
        .map_err(|e| type_error(&format!("invalid station: {e}")))?;
    let events = core_meridian_transits(
        source,
        parse_transit_body(body)?,
        &station.to_core(),
        instant(start_unix_us),
        instant(end_unix_us),
        step_s,
        tolerance_s,
    )
    .map_err(engine_error)?;
    let out: Vec<TransitEventJs> = events
        .into_iter()
        .map(|event| TransitEventJs {
            time_unix_us: event.time.unix_microseconds(),
            kind: culmination_label(event.kind),
            altitude_deg: event.altitude_deg,
        })
        .collect();
    serde_wasm_bindgen::to_value(&out).map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name = meridianTransits)]
pub fn meridian_transits(
    body: &str,
    station: JsValue,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    transits_with_source(
        EphemerisSource::Analytic,
        body,
        station,
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}

#[wasm_bindgen(js_name = meridianTransitsSpk)]
pub fn meridian_transits_spk(
    spk: &Spk,
    body: &str,
    station: JsValue,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    transits_with_source(
        source_spk(spk),
        body,
        station,
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}

fn eclipses_with_source(
    source: EphemerisSource<'_>,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    let events = core_lunar_solar_eclipses(
        source,
        instant(start_unix_us),
        instant(end_unix_us),
        step_s,
        tolerance_s,
    )
    .map_err(engine_error)?;
    let out: Vec<EclipseEventJs> = events
        .into_iter()
        .map(|event| EclipseEventJs {
            time_maximum_unix_us: event.time_maximum.unix_microseconds(),
            kind: eclipse_label(event.kind),
            magnitude: event.magnitude,
            moon_latitude_deg: event.moon_latitude_deg,
            gamma: event.gamma,
            uncertain: event.uncertain,
        })
        .collect();
    serde_wasm_bindgen::to_value(&out).map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name = lunarSolarEclipses)]
pub fn lunar_solar_eclipses(
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    eclipses_with_source(
        EphemerisSource::Analytic,
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}

#[wasm_bindgen(js_name = lunarSolarEclipsesSpk)]
pub fn lunar_solar_eclipses_spk(
    spk: &Spk,
    start_unix_us: i64,
    end_unix_us: i64,
    step_s: f64,
    tolerance_s: f64,
) -> Result<JsValue, JsValue> {
    eclipses_with_source(
        source_spk(spk),
        start_unix_us,
        end_unix_us,
        step_s,
        tolerance_s,
    )
}
