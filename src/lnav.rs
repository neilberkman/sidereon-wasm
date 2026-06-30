//! GPS LNAV navigation-message bit-level codec.
//!
//! Thin wrapper over `sidereon_core::navigation::lnav`. The IS-GPS-200 subframe
//! layout, per-field scaling and range checks, parity algorithm, and TLM/HOW
//! framing all live in the crate; this layer only decodes the JS parameter
//! object into the engine's typed inputs, hands bit buffers straight through, and
//! re-encodes the decoded parameters. Bit buffers are `Uint8Array`s of `0`/`1`
//! values: a 30-bit word, a 300-bit subframe, or 24 source data bits.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::navigation::lnav::{
    decode as core_decode, encode as core_encode, parity as core_parity,
    parity_valid as core_parity_valid, subframe_id as core_subframe_id, tow as core_tow,
    LnavDecoded as CoreLnavDecoded, LnavError, LnavNumber, LnavOptions, LnavParams,
};

use crate::error::engine_error;

/// Map the (non-`Display`) LNAV codec error onto a thrown JS `Error`.
fn lnav_err(error: LnavError) -> JsValue {
    let message = match error {
        LnavError::OutOfRange { field, value } => {
            format!("LNAV field {} out of range: {value:?}", field.name())
        }
        LnavError::ParityFailed { subframe, word } => {
            format!("LNAV parity failed at subframe {subframe} word {word}")
        }
        LnavError::BadWordLength { expected, actual } => {
            format!("LNAV word has {actual} data bits, expected {expected}")
        }
        LnavError::BadSubframeLength { subframe } => {
            format!("LNAV subframe {subframe} has the wrong bit length")
        }
    };
    engine_error(message)
}

fn int(value: f64) -> LnavNumber {
    LnavNumber::Int(value as i64)
}

fn flt(value: f64) -> LnavNumber {
    LnavNumber::Float(value)
}

/// JS-side LNAV clock/ephemeris parameters (engineering units). Integer fields
/// (week number, codes, health, IODC/IODE, fit-interval flag, AODO) cross as the
/// engine's integer-typed values; the scaled fields cross as floats.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LnavParamsInput {
    week_number: f64,
    l2_code: f64,
    l2_p_data_flag: f64,
    ura_index: f64,
    sv_health: f64,
    iodc: f64,
    tgd: f64,
    toc: f64,
    af0: f64,
    af1: f64,
    af2: f64,
    iode: f64,
    crs: f64,
    delta_n: f64,
    m0: f64,
    cuc: f64,
    eccentricity: f64,
    cus: f64,
    sqrt_a: f64,
    toe: f64,
    fit_interval_flag: f64,
    aodo: f64,
    cic: f64,
    omega0: f64,
    cis: f64,
    i0: f64,
    crc: f64,
    omega: f64,
    omega_dot: f64,
    idot: f64,
}

impl LnavParamsInput {
    fn to_core(&self) -> LnavParams {
        LnavParams {
            week_number: int(self.week_number),
            l2_code: int(self.l2_code),
            l2_p_data_flag: int(self.l2_p_data_flag),
            ura_index: int(self.ura_index),
            sv_health: int(self.sv_health),
            iodc: int(self.iodc),
            tgd: flt(self.tgd),
            toc: flt(self.toc),
            af0: flt(self.af0),
            af1: flt(self.af1),
            af2: flt(self.af2),
            iode: int(self.iode),
            crs: flt(self.crs),
            delta_n: flt(self.delta_n),
            m0: flt(self.m0),
            cuc: flt(self.cuc),
            eccentricity: flt(self.eccentricity),
            cus: flt(self.cus),
            sqrt_a: flt(self.sqrt_a),
            toe: flt(self.toe),
            fit_interval_flag: int(self.fit_interval_flag),
            aodo: int(self.aodo),
            cic: flt(self.cic),
            omega0: flt(self.omega0),
            cis: flt(self.cis),
            i0: flt(self.i0),
            crc: flt(self.crc),
            omega: flt(self.omega),
            omega_dot: flt(self.omega_dot),
            idot: flt(self.idot),
        }
    }
}

/// JS-side TLM/HOW options for [`lnavEncode`]. Every field is an integer and
/// defaults to `0` when omitted.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LnavOptionsInput {
    tow: f64,
    alert: f64,
    anti_spoof: f64,
    integrity: f64,
    tlm_message: f64,
}

impl LnavOptionsInput {
    fn to_core(&self) -> LnavOptions {
        LnavOptions {
            tow: int(self.tow),
            alert: int(self.alert),
            anti_spoof: int(self.anti_spoof),
            integrity: int(self.integrity),
            tlm_message: int(self.tlm_message),
        }
    }
}

/// The three 300-bit LNAV subframes produced by [`lnavEncode`].
#[wasm_bindgen]
pub struct LnavSubframes {
    subframe1: Vec<u8>,
    subframe2: Vec<u8>,
    subframe3: Vec<u8>,
}

#[wasm_bindgen]
impl LnavSubframes {
    /// Subframe 1 (clock/health) as a 300-element `Uint8Array` of `0`/`1` bits.
    #[wasm_bindgen(getter)]
    pub fn subframe1(&self) -> Vec<u8> {
        self.subframe1.clone()
    }

    /// Subframe 2 (ephemeris part 1) as a 300-element `Uint8Array`.
    #[wasm_bindgen(getter)]
    pub fn subframe2(&self) -> Vec<u8> {
        self.subframe2.clone()
    }

    /// Subframe 3 (ephemeris part 2) as a 300-element `Uint8Array`.
    #[wasm_bindgen(getter)]
    pub fn subframe3(&self) -> Vec<u8> {
        self.subframe3.clone()
    }
}

/// Decoded LNAV clock and ephemeris parameters (engineering units). Integer
/// fields are recovered exactly; scaled fields are the transmitted integer times
/// the IS-GPS-200 LSB. `l2_p_data_flag` is encode-only and not recovered.
#[wasm_bindgen]
pub struct LnavDecoded {
    inner: CoreLnavDecoded,
}

#[wasm_bindgen]
impl LnavDecoded {
    /// GPS week number.
    #[wasm_bindgen(getter, js_name = weekNumber)]
    pub fn week_number(&self) -> f64 {
        self.inner.week_number as f64
    }

    /// L2 code indicator.
    #[wasm_bindgen(getter, js_name = l2Code)]
    pub fn l2_code(&self) -> f64 {
        self.inner.l2_code as f64
    }

    /// User range accuracy index.
    #[wasm_bindgen(getter, js_name = uraIndex)]
    pub fn ura_index(&self) -> f64 {
        self.inner.ura_index as f64
    }

    /// SV health bits.
    #[wasm_bindgen(getter, js_name = svHealth)]
    pub fn sv_health(&self) -> f64 {
        self.inner.sv_health as f64
    }

    /// Issue of data, clock.
    #[wasm_bindgen(getter)]
    pub fn iodc(&self) -> f64 {
        self.inner.iodc as f64
    }

    /// Group delay differential, seconds.
    #[wasm_bindgen(getter)]
    pub fn tgd(&self) -> f64 {
        self.inner.tgd
    }

    /// Clock reference time, seconds.
    #[wasm_bindgen(getter)]
    pub fn toc(&self) -> f64 {
        self.inner.toc as f64
    }

    /// Clock bias coefficient, seconds.
    #[wasm_bindgen(getter)]
    pub fn af0(&self) -> f64 {
        self.inner.af0
    }

    /// Clock drift coefficient, seconds per second.
    #[wasm_bindgen(getter)]
    pub fn af1(&self) -> f64 {
        self.inner.af1
    }

    /// Clock drift-rate coefficient, seconds per second squared.
    #[wasm_bindgen(getter)]
    pub fn af2(&self) -> f64 {
        self.inner.af2
    }

    /// Issue of data, ephemeris.
    #[wasm_bindgen(getter)]
    pub fn iode(&self) -> f64 {
        self.inner.iode as f64
    }

    /// Sine harmonic correction to orbit radius, metres.
    #[wasm_bindgen(getter)]
    pub fn crs(&self) -> f64 {
        self.inner.crs
    }

    /// Mean motion difference, radians per second.
    #[wasm_bindgen(getter, js_name = deltaN)]
    pub fn delta_n(&self) -> f64 {
        self.inner.delta_n
    }

    /// Mean anomaly at reference time, radians.
    #[wasm_bindgen(getter)]
    pub fn m0(&self) -> f64 {
        self.inner.m0
    }

    /// Cosine harmonic correction to argument of latitude, radians.
    #[wasm_bindgen(getter)]
    pub fn cuc(&self) -> f64 {
        self.inner.cuc
    }

    /// Eccentricity.
    #[wasm_bindgen(getter)]
    pub fn eccentricity(&self) -> f64 {
        self.inner.eccentricity
    }

    /// Sine harmonic correction to argument of latitude, radians.
    #[wasm_bindgen(getter)]
    pub fn cus(&self) -> f64 {
        self.inner.cus
    }

    /// Square root of the semi-major axis, sqrt(metres).
    #[wasm_bindgen(getter, js_name = sqrtA)]
    pub fn sqrt_a(&self) -> f64 {
        self.inner.sqrt_a
    }

    /// Ephemeris reference time, seconds.
    #[wasm_bindgen(getter)]
    pub fn toe(&self) -> f64 {
        self.inner.toe as f64
    }

    /// Fit-interval flag.
    #[wasm_bindgen(getter, js_name = fitIntervalFlag)]
    pub fn fit_interval_flag(&self) -> f64 {
        self.inner.fit_interval_flag as f64
    }

    /// Age of data offset.
    #[wasm_bindgen(getter)]
    pub fn aodo(&self) -> f64 {
        self.inner.aodo as f64
    }

    /// Cosine harmonic correction to inclination, radians.
    #[wasm_bindgen(getter)]
    pub fn cic(&self) -> f64 {
        self.inner.cic
    }

    /// Longitude of ascending node at weekly epoch, radians.
    #[wasm_bindgen(getter)]
    pub fn omega0(&self) -> f64 {
        self.inner.omega0
    }

    /// Sine harmonic correction to inclination, radians.
    #[wasm_bindgen(getter)]
    pub fn cis(&self) -> f64 {
        self.inner.cis
    }

    /// Inclination at reference time, radians.
    #[wasm_bindgen(getter)]
    pub fn i0(&self) -> f64 {
        self.inner.i0
    }

    /// Cosine harmonic correction to orbit radius, metres.
    #[wasm_bindgen(getter)]
    pub fn crc(&self) -> f64 {
        self.inner.crc
    }

    /// Argument of perigee, radians.
    #[wasm_bindgen(getter)]
    pub fn omega(&self) -> f64 {
        self.inner.omega
    }

    /// Rate of right ascension, radians per second.
    #[wasm_bindgen(getter, js_name = omegaDot)]
    pub fn omega_dot(&self) -> f64 {
        self.inner.omega_dot
    }

    /// Rate of inclination, radians per second.
    #[wasm_bindgen(getter)]
    pub fn idot(&self) -> f64 {
        self.inner.idot
    }
}

/// Time-of-week count from a hand-over word or a full subframe.
///
/// `bits` is a 30-bit HOW word or a 300-bit subframe (whose word 2 is the HOW),
/// as a `Uint8Array` of `0`/`1`. Returns the 17-bit TOW count, or `undefined`
/// for any other length. Delegates to `sidereon_core::navigation::lnav::tow`.
#[wasm_bindgen(js_name = lnavTow)]
pub fn lnav_tow(bits: &[u8]) -> Option<u64> {
    core_tow(bits)
}

/// Subframe ID from a hand-over word or a full subframe.
///
/// `bits` is a 30-bit HOW word or a 300-bit subframe. Returns the 3-bit subframe
/// ID, or `undefined` for any other length. Delegates to
/// `sidereon_core::navigation::lnav::subframe_id`.
#[wasm_bindgen(js_name = lnavSubframeId)]
pub fn lnav_subframe_id(bits: &[u8]) -> Option<u64> {
    core_subframe_id(bits)
}

/// The six parity bits `[D25..D30]` of a word from its 24 source data bits.
///
/// `data24` is the 24 source data bits (most significant first, before the
/// `D30*` complementation). `d29Prev` / `d30Prev` are the previous word's two
/// trailing parity bits. Delegates to `sidereon_core::navigation::lnav::parity`.
#[wasm_bindgen(js_name = lnavParity)]
pub fn lnav_parity(data24: &[u8], d29_prev: u8, d30_prev: u8) -> Result<Vec<u8>, JsValue> {
    core_parity(data24, d29_prev, d30_prev)
        .map(|bits| bits.to_vec())
        .map_err(lnav_err)
}

/// Verify the parity of a single 30-bit word.
///
/// `word30` is the 30-bit word as transmitted (data bits possibly complemented
/// by `D30*`, then 6 received parity bits). `d29Prev` / `d30Prev` are the
/// previous word's trailing parity bits. Delegates to
/// `sidereon_core::navigation::lnav::parity_valid`.
#[wasm_bindgen(js_name = lnavParityValid)]
pub fn lnav_parity_valid(word30: &[u8], d29_prev: u8, d30_prev: u8) -> bool {
    core_parity_valid(word30, d29_prev, d30_prev)
}

/// Encode clock and ephemeris parameters into LNAV subframes 1-3.
///
/// `params` is the engineering-unit parameter object and `options` the optional
/// TLM/HOW object (omitted fields default to `0`). Returns the three 300-bit
/// subframes. Throws an `Error` on an out-of-range field. Delegates to
/// `sidereon_core::navigation::lnav::encode`.
#[wasm_bindgen(js_name = lnavEncode)]
pub fn lnav_encode(params: JsValue, options: JsValue) -> Result<LnavSubframes, JsValue> {
    let params: LnavParamsInput = serde_wasm_bindgen::from_value(params)
        .map_err(|e| crate::error::type_error(&format!("invalid LNAV params: {e}")))?;
    let options: LnavOptionsInput = if options.is_undefined() || options.is_null() {
        LnavOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| crate::error::type_error(&format!("invalid LNAV options: {e}")))?
    };
    let [sf1, sf2, sf3] = core_encode(&params.to_core(), &options.to_core()).map_err(lnav_err)?;
    Ok(LnavSubframes {
        subframe1: sf1,
        subframe2: sf2,
        subframe3: sf3,
    })
}

/// Decode LNAV subframes 1-3 back into engineering-unit parameters.
///
/// Each subframe is a 300-bit `Uint8Array` of `0`/`1`. Parity is verified on all
/// 30 words first; a failure throws an `Error`. Delegates to
/// `sidereon_core::navigation::lnav::decode`.
#[wasm_bindgen(js_name = lnavDecode)]
pub fn lnav_decode(sf1: &[u8], sf2: &[u8], sf3: &[u8]) -> Result<LnavDecoded, JsValue> {
    let inner = core_decode(sf1, sf2, sf3).map_err(lnav_err)?;
    Ok(LnavDecoded { inner })
}
