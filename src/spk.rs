//! JPL/NAIF SPK (`.bsp`) binary ephemeris kernels: load an in-memory kernel and
//! query a body or spacecraft state (position km, velocity km/s) at an epoch.
//!
//! The reader resolves the segment chain connecting two NAIF body ids and
//! evaluates SPK Types 2 and 3 (Chebyshev) and Type 21 (Extended Modified
//! Difference Arrays). Every number returned is exactly what `sidereon-core`'s
//! `astro::spk` reader produces; this binding only marshals bytes in and the
//! resolved state out.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::spk::{Spk as CoreSpk, SpkSegmentDescriptor};

use crate::error::{engine_error, range_error};

/// A descriptor for one SPK segment as recorded in the DAF summary, in summary
/// order. Read via [`Spk.segments`].
#[wasm_bindgen]
pub struct SpkSegment {
    name: String,
    start_et: f64,
    stop_et: f64,
    target: i32,
    center: i32,
    frame: i32,
    data_type: i32,
}

impl From<&SpkSegmentDescriptor> for SpkSegment {
    fn from(d: &SpkSegmentDescriptor) -> Self {
        Self {
            name: d.name.clone(),
            start_et: d.start_et,
            stop_et: d.stop_et,
            target: d.target,
            center: d.center,
            frame: d.frame,
            data_type: d.data_type,
        }
    }
}

#[wasm_bindgen]
impl SpkSegment {
    /// Segment comment/name from the DAF summary record.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Coverage start, ET/TDB seconds past J2000.
    #[wasm_bindgen(getter, js_name = startEt)]
    pub fn start_et(&self) -> f64 {
        self.start_et
    }

    /// Coverage stop, ET/TDB seconds past J2000.
    #[wasm_bindgen(getter, js_name = stopEt)]
    pub fn stop_et(&self) -> f64 {
        self.stop_et
    }

    /// NAIF target body id.
    #[wasm_bindgen(getter)]
    pub fn target(&self) -> i32 {
        self.target
    }

    /// NAIF center body id.
    #[wasm_bindgen(getter)]
    pub fn center(&self) -> i32 {
        self.center
    }

    /// NAIF reference-frame id.
    #[wasm_bindgen(getter)]
    pub fn frame(&self) -> i32 {
        self.frame
    }

    /// SPK segment data type (2, 3, or 21 for the supported evaluators).
    #[wasm_bindgen(getter, js_name = dataType)]
    pub fn data_type(&self) -> i32 {
        self.data_type
    }
}

/// The state of a target relative to a center, resolved from an SPK query.
/// Returned by [`Spk.state`].
#[wasm_bindgen]
pub struct SpkState {
    target: i32,
    center: i32,
    position: Vec<f64>,
    velocity: Option<Vec<f64>>,
    frame: i32,
}

#[wasm_bindgen]
impl SpkState {
    /// NAIF target body id for the returned relative state.
    #[wasm_bindgen(getter)]
    pub fn target(&self) -> i32 {
        self.target
    }

    /// NAIF center body id for the returned relative state.
    #[wasm_bindgen(getter)]
    pub fn center(&self) -> i32 {
        self.center
    }

    /// Position of the target relative to the center as a `Float64Array`
    /// `[x, y, z]`, kilometres.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.position.clone()
    }

    /// Velocity of the target relative to the center as a `Float64Array`
    /// `[vx, vy, vz]`, kilometres per second, or `undefined` when the resolved
    /// path includes a Type-2 segment (which stores position only).
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Option<Vec<f64>> {
        self.velocity.clone()
    }

    /// NAIF reference-frame id shared by all segments in the resolved path.
    #[wasm_bindgen(getter)]
    pub fn frame(&self) -> i32 {
        self.frame
    }
}

/// A parsed in-memory JPL/NAIF SPK kernel.
///
/// Construct from the raw `.bsp` bytes (a `Uint8Array`), then query states with
/// [`Spk.state`] or inspect the segment table with [`Spk.segments`].
///
/// ```js
/// import init, { Spk } from "@neilberkman/sidereon";
/// await init();
/// const spk = new Spk(new Uint8Array(bspBytes));
/// const state = spk.state(20000433, 10, 757339200.0);
/// state.positionKm; // Float64Array [x, y, z] km
/// state.velocityKmS; // Float64Array [vx, vy, vz] km/s (or undefined)
/// ```
#[wasm_bindgen]
pub struct Spk {
    inner: CoreSpk,
}

impl Spk {
    pub(crate) fn core(&self) -> &CoreSpk {
        &self.inner
    }
}

#[wasm_bindgen]
impl Spk {
    /// Parse an SPK/DAF kernel from its raw bytes (the full, decompressed
    /// `.bsp` file). Throws an `Error` if the bytes are not a supported SPK
    /// kernel.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<Spk, JsValue> {
        let inner = CoreSpk::from_bytes(bytes).map_err(engine_error)?;
        Ok(Spk { inner })
    }

    /// Segment descriptors in DAF summary order.
    #[wasm_bindgen(getter)]
    pub fn segments(&self) -> Vec<SpkSegment> {
        self.inner.segments().iter().map(SpkSegment::from).collect()
    }

    /// Number of segments in the kernel.
    #[wasm_bindgen(getter, js_name = segmentCount)]
    pub fn segment_count(&self) -> usize {
        self.inner.segments().len()
    }

    /// The DAF internal name from the file record.
    #[wasm_bindgen(getter, js_name = internalName)]
    pub fn internal_name(&self) -> String {
        self.inner.file_record().internal_name.clone()
    }

    /// Query the state of NAIF `target` relative to NAIF `center` at `et`
    /// (ET/TDB seconds past J2000). Resolves the segment chain connecting the
    /// two bodies and evaluates SPK Types 2, 3, and 21.
    ///
    /// Throws a `RangeError` if `et` is not finite, and an `Error` if the bodies
    /// are absent, unconnected, out of coverage, or in an unsupported segment.
    #[wasm_bindgen]
    pub fn state(&self, target: i32, center: i32, et: f64) -> Result<SpkState, JsValue> {
        if !et.is_finite() {
            return Err(range_error("et must be a finite number"));
        }
        let state = self
            .inner
            .spk_state(target, center, et)
            .map_err(engine_error)?;
        Ok(SpkState {
            target: state.target,
            center: state.center,
            position: state.position_km.to_vec(),
            velocity: state.velocity_km_s.map(|v| v.to_vec()),
            frame: state.frame,
        })
    }
}
