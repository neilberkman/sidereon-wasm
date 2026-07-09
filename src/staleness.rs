//! Product-staleness graceful degradation and broadcast-ephemeris SPP with a
//! precise-to-broadcast fallback.
//!
//! This module wraps two `sidereon-core` capabilities, owning no modeling logic:
//!
//! 1. Product-staleness selection (`sidereon_core::staleness`): given a set of
//!    parsed IONEX or SP3 products and a requested epoch (or range), pick a
//!    usable product, degrading to the most-recent prior product within a
//!    configurable cap (IONEX diurnal shift, SP3 nearest-prior) instead of
//!    failing on a missing epoch. Every selection carries [`StalenessMetadata`]
//!    so a degraded answer is never substituted silently, and only a request past
//!    the cap fails, with a typed error.
//! 2. Broadcast-ephemeris SPP and fallback (`sidereon_core::positioning`):
//!    [`BroadcastEphemeris.solveBroadcast`] is the broadcast-only real-time /
//!    offline mode, and [`solveWithFallback`] prefers precise products through the
//!    staleness layer and falls back to broadcast, returning a sourced solution
//!    that names which source produced the fix and how stale it is.
//!
//! Idioms: products cross in as JS arrays of parsed handles (consumed, as
//! [`mergeSp3`](crate::merge_sp3) does); staleness metadata and the fix source
//! cross out as plain JS objects with a string `kind` discriminator; selection
//! and fallback failures throw a typed JS `Error` whose `name` is the variant and
//! whose `detail` carries the structured fields, so the reason is never lost.
//!
//! This layer is pure and no-network, like the core it wraps: it selects among
//! products the caller has already parsed. Fetching products is a per-binding
//! concern handled elsewhere in the data surface.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::ephemeris::Sp3 as CoreSp3;
use sidereon_core::positioning::{
    solve_broadcast as core_solve_broadcast, solve_with_fallback as core_solve_with_fallback,
    BroadcastReason, FallbackError, FixSource,
};
use sidereon_core::staleness::{
    select_ionex, select_ionex_over_range, select_sp3, select_sp3_over_range, DegradationKind,
    SelectionError, StalenessMetadata, StalenessPolicy,
};
use sidereon_core::Error as CoreError;
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, require_finite, type_error};
use crate::ionex::{slant_delay_deg, Ionex};
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::Sp3;
use crate::spp::{self, SppSolution};

// --- Staleness policy -------------------------------------------------------

/// Optional staleness cap object: at most one of `maxStalenessS` / `maxStalenessDays`.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PolicyInput {
    max_staleness_s: Option<f64>,
    max_staleness_days: Option<f64>,
}

/// Parse the optional `policy` argument into a core [`StalenessPolicy`]. Absent
/// (`undefined` / `null`) is the engine default cap (3 days). A non-finite or
/// negative cap is not rejected here: the core selection layer surfaces it as the
/// typed `InvalidPolicy` error, the single place that decision lives.
fn parse_policy(policy: JsValue) -> Result<StalenessPolicy, JsValue> {
    if policy.is_undefined() || policy.is_null() {
        return Ok(StalenessPolicy::default());
    }
    let parsed: PolicyInput = serde_wasm_bindgen::from_value(policy)
        .map_err(|e| type_error(&format!("invalid staleness policy: {e}")))?;
    match (parsed.max_staleness_s, parsed.max_staleness_days) {
        (Some(_), Some(_)) => Err(type_error(
            "staleness policy must set at most one of maxStalenessS or maxStalenessDays",
        )),
        (Some(seconds), None) => Ok(StalenessPolicy::seconds(seconds)),
        (None, Some(days)) => Ok(StalenessPolicy::days(days)),
        (None, None) => Ok(StalenessPolicy::default()),
    }
}

// --- Plain-object metadata --------------------------------------------------

/// Stable lower-camel string for a degradation kind.
fn degradation_kind_str(kind: DegradationKind) -> &'static str {
    match kind {
        DegradationKind::Exact => "exact",
        DegradationKind::NearestPrior => "nearestPrior",
        DegradationKind::DiurnalShift => "diurnalShift",
    }
}

/// Staleness metadata as a plain JS object. `kind` is `"exact" | "nearestPrior"
/// | "diurnalShift"`; epochs are J2000 seconds; `stalenessS` is `requested -
/// source` and never negative.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StalenessMetadataJs {
    kind: &'static str,
    requested_epoch_j2000_s: f64,
    source_epoch_j2000_s: f64,
    staleness_s: f64,
    staleness_days: f64,
}

impl From<StalenessMetadata> for StalenessMetadataJs {
    fn from(meta: StalenessMetadata) -> Self {
        Self {
            kind: degradation_kind_str(meta.kind),
            requested_epoch_j2000_s: meta.requested_epoch_j2000_s,
            source_epoch_j2000_s: meta.source_epoch_j2000_s,
            staleness_s: meta.staleness_s,
            staleness_days: meta.staleness_days,
        }
    }
}

/// Serialize a plain-object result, emitting `null` (not `undefined`) for an
/// absent `Option` field so a deliberately-absent provenance field (`staleness`,
/// `broadcastReason`) reads as `null` on the JS side, matching the TS types.
///
/// A serialization failure is surfaced (panic -> JS exception), never collapsed
/// to `null`: a silent `null` here would drop the very staleness/source metadata
/// this layer exists to attach. These plain structs (numbers, bools, strings,
/// nested options) do not fail to serialize in practice; the expect guards the
/// doctrine rather than a known failure mode.
fn to_js<T: Serialize>(value: &T) -> JsValue {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
    value
        .serialize(&serializer)
        .expect("serialize staleness/source metadata to a plain JS object")
}

fn metadata_to_js(meta: StalenessMetadata) -> JsValue {
    to_js(&StalenessMetadataJs::from(meta))
}

// --- Typed selection errors -------------------------------------------------

/// A selection failure as a plain object: the variant `name`, a human `message`,
/// and the structured fields for the variant. Mirrors `SelectionError`; the
/// metre/epoch fields are present only for the variants that carry them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectionErrorJs {
    name: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_epoch_j2000_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_epoch_j2000_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_epoch_j2000_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_epoch_j2000_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    staleness_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_staleness_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

impl SelectionErrorJs {
    fn new(name: &'static str, message: String) -> Self {
        Self {
            name,
            message,
            requested_epoch_j2000_s: None,
            start_epoch_j2000_s: None,
            end_epoch_j2000_s: None,
            source_epoch_j2000_s: None,
            staleness_s: None,
            max_staleness_s: None,
            context: None,
        }
    }
}

impl From<&SelectionError> for SelectionErrorJs {
    fn from(error: &SelectionError) -> Self {
        let message = error.to_string();
        match error {
            SelectionError::EmptyProductSet => Self::new("EmptyProductSet", message),
            SelectionError::InvalidRange {
                start_epoch_j2000_s,
                end_epoch_j2000_s,
            } => {
                let mut js = Self::new("InvalidRange", message);
                js.start_epoch_j2000_s = Some(*start_epoch_j2000_s);
                js.end_epoch_j2000_s = Some(*end_epoch_j2000_s);
                js
            }
            SelectionError::NoPriorProduct {
                requested_epoch_j2000_s,
            } => {
                let mut js = Self::new("NoPriorProduct", message);
                js.requested_epoch_j2000_s = Some(*requested_epoch_j2000_s);
                js
            }
            SelectionError::BeyondStalenessCap {
                requested_epoch_j2000_s,
                source_epoch_j2000_s,
                staleness_s,
                max_staleness_s,
            } => {
                let mut js = Self::new("BeyondStalenessCap", message);
                js.requested_epoch_j2000_s = Some(*requested_epoch_j2000_s);
                js.source_epoch_j2000_s = Some(*source_epoch_j2000_s);
                js.staleness_s = Some(*staleness_s);
                js.max_staleness_s = Some(*max_staleness_s);
                js
            }
            SelectionError::InvalidProduct(_) => Self::new("InvalidProduct", message),
            SelectionError::InvalidPolicy { max_staleness_s } => {
                let mut js = Self::new("InvalidPolicy", message);
                js.max_staleness_s = Some(*max_staleness_s);
                js
            }
            SelectionError::Overflow { context } => {
                let mut js = Self::new("Overflow", message);
                js.context = Some((*context).to_string());
                js
            }
        }
    }
}

/// Build a typed JS `Error` from a selection failure: `error.name` is the variant
/// (e.g. `"BeyondStalenessCap"`) and `error.detail` carries the structured
/// fields, so the staleness reason is discriminable and never dropped.
fn selection_error(error: &SelectionError) -> JsValue {
    let detail = SelectionErrorJs::from(error);
    let js_error = js_sys::Error::new(&detail.message);
    js_error.set_name(detail.name);
    let value: JsValue = js_error.into();
    attach_detail(&value, &detail);
    value
}

/// A fallback failure as a plain object: which path failed (`name`) and the
/// underlying solve error (`message`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FallbackErrorJs {
    name: &'static str,
    message: String,
}

/// Build a typed JS `Error` from a fallback failure: `error.name` names which
/// path failed (`"PreciseSolveError"` / `"BroadcastSolveError"`) and `error.detail`
/// carries the same structured fields, mirroring [`selection_error`].
fn fallback_error(error: &FallbackError) -> JsValue {
    let name = match error {
        FallbackError::Precise(_) => "PreciseSolveError",
        FallbackError::Broadcast(_) => "BroadcastSolveError",
    };
    let message = error.to_string();
    let js_error = js_sys::Error::new(&message);
    js_error.set_name(name);
    let value: JsValue = js_error.into();
    attach_detail(&value, &FallbackErrorJs { name, message });
    value
}

/// Attach a serialized `detail` object to a thrown JS `Error`, surfacing any
/// failure (panic -> JS exception) so a typed error never loses its structured
/// fields silently.
fn attach_detail<T: Serialize>(value: &JsValue, detail: &T) {
    let detail_value =
        serde_wasm_bindgen::to_value(detail).expect("serialize typed-error detail object");
    js_sys::Reflect::set(value, &JsValue::from_str("detail"), &detail_value)
        .expect("attach detail to typed error");
}

// --- IONEX selection --------------------------------------------------------

/// A staleness-selected IONEX product plus its [`StalenessMetadata`]. The inner
/// product is the present product for an exact selection or a diurnal-shifted
/// copy for a degraded one; either way `slantDelay` evaluates the standard slant
/// delay on it.
#[wasm_bindgen]
pub struct IonexSelection {
    inner: sidereon_core::atmosphere::Ionex,
    metadata: StalenessMetadata,
}

#[wasm_bindgen]
impl IonexSelection {
    /// The staleness metadata as a plain object (see [`StalenessMetadataJs`]).
    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> JsValue {
        metadata_to_js(self.metadata)
    }

    /// The usable IONEX product as a standalone [`Ionex`] handle: the present
    /// product for an exact selection, or the diurnal-shifted copy.
    #[wasm_bindgen(getter)]
    pub fn ionex(&self) -> Ionex {
        Ionex {
            inner: self.inner.clone(),
        }
    }

    /// Slant ionospheric group delay, positive metres, from the selected product.
    /// Same degree-valued geometry as [`Ionex.slantDelay`]; for an exact
    /// selection the result is bit-for-bit identical to evaluating the caller's
    /// product directly.
    #[wasm_bindgen(js_name = slantDelay)]
    pub fn slant_delay(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        azimuth_deg: f64,
        elevation_deg: f64,
        epoch_j2000_s: f64,
        frequency_hz: f64,
    ) -> Result<f64, JsValue> {
        slant_delay_deg(
            &self.inner,
            lat_deg,
            lon_deg,
            azimuth_deg,
            elevation_deg,
            epoch_j2000_s,
            frequency_hz,
        )
    }
}

/// Select an IONEX product usable at `requestedEpochJ2000S`, degrading to a
/// diurnal-shifted prior product within `policy`.
///
/// `products` is a JS array of parsed [`Ionex`] handles (consumed). `policy` is
/// optional (`{ maxStalenessS }` or `{ maxStalenessDays }`, default 3 days).
/// Throws a typed selection `Error` when no product fits the request and cap.
#[wasm_bindgen(js_name = selectIonex)]
pub fn select_ionex_js(
    products: Vec<Ionex>,
    requested_epoch_j2000_s: f64,
    policy: JsValue,
) -> Result<IonexSelection, JsValue> {
    require_finite(requested_epoch_j2000_s, "requestedEpochJ2000S")?;
    let policy = parse_policy(policy)?;
    let core: Vec<_> = products.into_iter().map(|p| p.inner).collect();
    let selection = select_ionex(&core, requested_epoch_j2000_s as i64, policy)
        .map_err(|e| selection_error(&e))?;
    Ok(IonexSelection {
        inner: selection.ionex().clone(),
        metadata: selection.metadata(),
    })
}

/// Select an IONEX product usable across `[startEpochJ2000S, endEpochJ2000S]`.
/// See [`selectIonex`] for the single-epoch case and the arguments.
#[wasm_bindgen(js_name = selectIonexOverRange)]
pub fn select_ionex_over_range_js(
    products: Vec<Ionex>,
    start_epoch_j2000_s: f64,
    end_epoch_j2000_s: f64,
    policy: JsValue,
) -> Result<IonexSelection, JsValue> {
    require_finite(start_epoch_j2000_s, "startEpochJ2000S")?;
    require_finite(end_epoch_j2000_s, "endEpochJ2000S")?;
    let policy = parse_policy(policy)?;
    let core: Vec<_> = products.into_iter().map(|p| p.inner).collect();
    let selection = select_ionex_over_range(
        &core,
        start_epoch_j2000_s as i64,
        end_epoch_j2000_s as i64,
        policy,
    )
    .map_err(|e| selection_error(&e))?;
    Ok(IonexSelection {
        inner: selection.ionex().clone(),
        metadata: selection.metadata(),
    })
}

// --- SP3 selection ----------------------------------------------------------

/// A staleness-selected SP3 product plus its [`StalenessMetadata`].
#[wasm_bindgen]
pub struct Sp3Selection {
    inner: CoreSp3,
    metadata: StalenessMetadata,
}

#[wasm_bindgen]
impl Sp3Selection {
    /// The staleness metadata as a plain object (see [`StalenessMetadataJs`]).
    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> JsValue {
        metadata_to_js(self.metadata)
    }

    /// The selected SP3 product as a standalone [`Sp3`] handle, exposing the full
    /// SP3 query / solve surface.
    #[wasm_bindgen(getter)]
    pub fn sp3(&self) -> Sp3 {
        Sp3 {
            inner: self.inner.clone(),
        }
    }

    /// Interpolate `satellite` at `queryJ2000S` on the selected product, returning
    /// a plain object `{ positionM: [x, y, z], clockS: number | null }`. Delegates
    /// to the reference interpolation, so an exact selection matches the caller's
    /// product bit-for-bit. Throws a `TypeError` for an unknown satellite and an
    /// `Error` for a query in a coverage gap.
    #[wasm_bindgen(js_name = positionAtJ2000Seconds)]
    pub fn position_at_j2000_seconds(
        &self,
        satellite: &str,
        query_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        require_finite(query_j2000_s, "queryJ2000S")?;
        let sat = satellite
            .parse::<GnssSatelliteId>()
            .map_err(|e| type_error(&format!("invalid satellite token {satellite:?}: {e}")))?;
        let state = self
            .inner
            .position_at_j2000_seconds(sat, query_j2000_s)
            .map_err(|e| match e {
                CoreError::UnknownSatellite(id) => {
                    type_error(&format!("satellite {id} is not in the product"))
                }
                other => engine_error(format!(
                    "interpolation at j2000 second {query_j2000_s}: {other}"
                )),
            })?;
        let value = Sp3StateJs {
            position_m: state.position.as_array().to_vec(),
            clock_s: state.clock_s,
        };
        Ok(to_js(&value))
    }
}

/// Interpolated SP3 position and clock as a plain JS object.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3StateJs {
    position_m: Vec<f64>,
    clock_s: Option<f64>,
}

/// Select an SP3 product usable at `requestedEpochJ2000S`, degrading to the
/// most-recent prior product within `policy`. See [`selectIonex`] for the
/// `products` / `policy` conventions.
#[wasm_bindgen(js_name = selectSp3)]
pub fn select_sp3_js(
    products: Vec<Sp3>,
    requested_epoch_j2000_s: f64,
    policy: JsValue,
) -> Result<Sp3Selection, JsValue> {
    require_finite(requested_epoch_j2000_s, "requestedEpochJ2000S")?;
    let policy = parse_policy(policy)?;
    let core: Vec<_> = products.into_iter().map(|p| p.inner).collect();
    let selection =
        select_sp3(&core, requested_epoch_j2000_s, policy).map_err(|e| selection_error(&e))?;
    Ok(Sp3Selection {
        inner: selection.sp3().clone(),
        metadata: selection.metadata(),
    })
}

/// Select an SP3 product usable across `[startEpochJ2000S, endEpochJ2000S]`.
#[wasm_bindgen(js_name = selectSp3OverRange)]
pub fn select_sp3_over_range_js(
    products: Vec<Sp3>,
    start_epoch_j2000_s: f64,
    end_epoch_j2000_s: f64,
    policy: JsValue,
) -> Result<Sp3Selection, JsValue> {
    require_finite(start_epoch_j2000_s, "startEpochJ2000S")?;
    require_finite(end_epoch_j2000_s, "endEpochJ2000S")?;
    let policy = parse_policy(policy)?;
    let core: Vec<_> = products.into_iter().map(|p| p.inner).collect();
    let selection = select_sp3_over_range(&core, start_epoch_j2000_s, end_epoch_j2000_s, policy)
        .map_err(|e| selection_error(&e))?;
    Ok(Sp3Selection {
        inner: selection.sp3().clone(),
        metadata: selection.metadata(),
    })
}

// --- Broadcast SPP + fallback ----------------------------------------------

/// The fix source of a [`SourcedSolution`] as a plain JS object: `kind` is
/// `"precise" | "broadcast"`; `staleness` is the precise product's metadata for a
/// precise fix and `null` for a broadcast fix; `broadcastReason` explains why
/// broadcast was used (and is `null` for a precise fix).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FixSourceJs {
    kind: &'static str,
    is_precise: bool,
    is_broadcast: bool,
    is_precise_exact: bool,
    staleness: Option<StalenessMetadataJs>,
    broadcast_reason: Option<BroadcastReasonJs>,
}

/// Why the broadcast path produced a fix, as a plain JS object.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastReasonJs {
    kind: &'static str,
    /// The precise selection's rejection, for `"preciseUnavailable"`.
    selection_error: Option<SelectionErrorJs>,
    /// The staleness of the precise product that was tried then fell back, for
    /// `"preciseDegradedUnusable"`.
    attempted_staleness: Option<StalenessMetadataJs>,
    /// The precise solve error that triggered the fallback, for
    /// `"preciseDegradedUnusable"`.
    precise_error: Option<String>,
}

impl From<&FixSource> for FixSourceJs {
    fn from(source: &FixSource) -> Self {
        match source {
            FixSource::Precise(meta) => Self {
                kind: "precise",
                is_precise: true,
                is_broadcast: false,
                is_precise_exact: source.is_precise_exact(),
                staleness: Some(StalenessMetadataJs::from(*meta)),
                broadcast_reason: None,
            },
            FixSource::Broadcast(reason) => Self {
                kind: "broadcast",
                is_precise: false,
                is_broadcast: true,
                is_precise_exact: false,
                staleness: None,
                broadcast_reason: Some(BroadcastReasonJs::from(reason)),
            },
        }
    }
}

impl From<&BroadcastReason> for BroadcastReasonJs {
    fn from(reason: &BroadcastReason) -> Self {
        match reason {
            BroadcastReason::PreciseUnavailable(error) => Self {
                kind: "preciseUnavailable",
                selection_error: Some(SelectionErrorJs::from(error)),
                attempted_staleness: None,
                precise_error: None,
            },
            BroadcastReason::PreciseDegradedUnusable { staleness, error } => Self {
                kind: "preciseDegradedUnusable",
                selection_error: None,
                attempted_staleness: Some(StalenessMetadataJs::from(*staleness)),
                precise_error: Some(error.to_string()),
            },
        }
    }
}

/// A receiver solution paired with the provenance of the ephemeris that produced
/// it. Returned by [`solveWithFallback`].
#[wasm_bindgen]
pub struct SourcedSolution {
    solution: SppSolution,
    source: FixSource,
}

#[wasm_bindgen]
impl SourcedSolution {
    /// The solved receiver position / clock as an [`SppSolution`].
    #[wasm_bindgen(getter)]
    pub fn solution(&self) -> SppSolution {
        SppSolution::from_inner(self.solution.inner.clone())
    }

    /// Which ephemeris source produced the fix, with its staleness / rejection
    /// provenance, as a plain object (see [`FixSourceJs`]).
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> JsValue {
        to_js(&FixSourceJs::from(&self.source))
    }
}

#[wasm_bindgen]
impl BroadcastEphemeris {
    /// Solve a receiver position from broadcast ephemeris ALONE: the supported
    /// real-time / offline single-point-positioning mode. `request` is the same
    /// SPP request object [`Sp3.solveSpp`] takes. The result is identical to
    /// solving against this broadcast store as an ephemeris source.
    #[wasm_bindgen(js_name = solveBroadcast)]
    pub fn solve_broadcast(&self, request: JsValue) -> Result<SppSolution, JsValue> {
        let (inputs, with_geodetic) = spp::build_inputs(request)?;
        let solution =
            core_solve_broadcast(&self.inner, &inputs, with_geodetic).map_err(engine_error)?;
        Ok(SppSolution::from_inner(solution))
    }

    /// Run fault detection and exclusion against this broadcast ephemeris.
    ///
    /// `request` is the SPP solve request plus the RAIM/exclusion options, the
    /// same object accepted by `Sp3.fde`. Delegates to
    /// `sidereon_core::quality::fde_spp` over the broadcast store.
    #[wasm_bindgen(js_name = fde)]
    pub fn fde(&self, request: JsValue) -> Result<crate::qc::FdeSolution, JsValue> {
        crate::qc::fde(&self.inner, request)
    }
}

/// Solve a receiver position, preferring precise SP3 products and falling back to
/// broadcast ephemeris, reporting which source was used and how stale it is.
///
/// `precise` is a JS array of parsed [`Sp3`] handles (consumed); the staleness
/// layer selects among them at the request's receive epoch. `broadcast` is the
/// broadcast store used when no precise product covers the epoch within `policy`.
/// `request` is the SPP request object [`Sp3.solveSpp`] takes. Returns a
/// [`SourcedSolution`]; throws a typed fallback `Error` if the chosen path's
/// solve fails.
#[wasm_bindgen(js_name = solveWithFallback)]
pub fn solve_with_fallback_js(
    precise: Vec<Sp3>,
    broadcast: &BroadcastEphemeris,
    request: JsValue,
    policy: JsValue,
) -> Result<SourcedSolution, JsValue> {
    let policy = parse_policy(policy)?;
    let (inputs, with_geodetic) = spp::build_inputs(request)?;
    let core: Vec<_> = precise.into_iter().map(|p| p.inner).collect();
    let sourced = core_solve_with_fallback(&core, &broadcast.inner, &inputs, policy, with_geodetic)
        .map_err(|e| fallback_error(&e))?;
    Ok(SourcedSolution {
        solution: SppSolution::from_inner(sourced.solution),
        source: sourced.source,
    })
}
