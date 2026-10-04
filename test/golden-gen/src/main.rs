//! Reproduce the WASM test scenarios natively against sidereon-core and print
//! their inputs and expected values as JSON (`test/fixtures/core_goldens.json`).
//!
//! Each section builds the same inputs the corresponding test hands the binding,
//! through the same core entry point the binding delegates to, so a test that
//! reads its expected values from this file checks the binding's marshalling
//! against the engine. Where a test synthesizes observations, the synthesis is
//! done here and its output is part of the section, so the test and the engine
//! see identical inputs. Floating-point values are written as their IEEE-754
//! bit patterns (`"0x..."`).
//!
//! # Running it against a core commit
//!
//! The goldens belong to one sidereon-core commit. The generator refuses to run
//! unless `SIDEREON_CORE_REV` names that commit (7 to 40 hexadecimal digits),
//! and records it in the output's `source`. It does not check the name against
//! the code it was built with, so build it against a checkout of that commit:
//!
//! 1. Check out sidereon at the commit, say in `../sidereon-at-rev`.
//! 2. Set the `sidereon` and `sidereon-core` requirements in this crate's
//!    `Cargo.toml` to the version that checkout's `crates/sidereon-core/Cargo.toml`
//!    states, so the patch below applies.
//! 3. Run from the repository root, patching both crates to the checkout:
//!
//! ```text
//! SIDEREON_CORE_REV=<commit> cargo run --release \
//!     --manifest-path test/golden-gen/Cargo.toml \
//!     --config 'patch.crates-io.sidereon-core.path="../sidereon-at-rev/crates/sidereon-core"' \
//!     --config 'patch.crates-io.sidereon.path="../sidereon-at-rev/crates/sidereon"' \
//!     > test/fixtures/core_goldens.json
//! ```
//!
//! `Cargo.lock` pins every other dependency. Rerun after an engine change that
//! moves these values; never edit the output by hand.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use serde_json::{json, Value};

use sidereon_core::astro::time::civil::j2000_seconds;
use sidereon_core::atmosphere::troposphere::Met;
use sidereon_core::bias::BiasSet;
use sidereon_core::constants::C_M_S;
use sidereon_core::ephemeris::MmapPreciseEphemerisInterpolant;
use sidereon_core::ephemeris::{BroadcastEphemeris, Sp3};
use sidereon_core::fusion as core_fusion;
use sidereon_core::inertial as core_inertial;
use sidereon_core::observables::{
    emission_media_batch_at_j2000_s, predict, EmissionMediaBatch, EmissionMediaBatchOptions,
    ObservableTroposphereCorrection, PredictOptions,
};
use sidereon_core::positioning::{
    solve_static, solve_with_doppler_velocity, Corrections, DopplerObservation, EphemerisSource,
    KlobucharCoeffs, Observation, PseudorangeCode, QzssClock, SolveInputs, SolvePolicy,
    StaticEpoch, StaticSolveOptions, SurfaceMet, TroposphereModel,
};
use sidereon_core::ppp_corrections::CivilDateTime;
use sidereon_core::precise_positioning::{
    FixedAmbiguityOptions, FixedSolveConfig, FloatEpoch, FloatObservation, FloatSolveConfig,
    FloatSolveOptions, FloatState, MeasurementWeights, RangeCorrections, TropoMapping,
    TroposphereOptions,
};
use sidereon_core::quality::{raim_for_solution, RaimOptions, SolutionValidationOptions};
use sidereon_core::rinex::qc::RepairOptions;
use sidereon_core::velocity::{
    range_rate_to_doppler, solve as solve_velocity, VelocityObservable, VelocityObservation,
    VelocitySolveOptions,
};
use sidereon_core::{GnssSatelliteId, GnssSystem};

const OMEGA_E: f64 = 7.2921151467e-5;
const F_L1_HZ: f64 = 1_575_420_000.0;
const GRG_SP3: &str = "GRG0MGXFIN_20201760000_01D_15M_ORB.SP3";
const GBM_SP3: &str = "sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3";

fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures")
        .join(rel)
}

fn fixture_bytes(rel: &str) -> Vec<u8> {
    std::fs::read(fixture_path(rel)).unwrap_or_else(|e| panic!("read fixture {rel}: {e}"))
}

fn fixture_text(rel: &str) -> String {
    String::from_utf8(fixture_bytes(rel)).expect("fixture is UTF-8")
}

fn hex(value: f64) -> String {
    format!("0x{:016x}", value.to_bits())
}

fn hexes(values: &[f64]) -> Vec<String> {
    values.iter().copied().map(hex).collect()
}

fn from_hex(text: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(text.trim_start_matches("0x"), 16).expect("hex bits"))
}

fn mat3(matrix: &[[f64; 3]; 3]) -> Vec<String> {
    matrix.iter().flatten().copied().map(hex).collect()
}

fn mat4(matrix: &[[f64; 4]; 4]) -> Vec<String> {
    matrix.iter().flatten().copied().map(hex).collect()
}

fn load_sp3(rel: &str) -> Sp3 {
    sidereon::load_sp3(&fixture_bytes(rel)).expect("parse SP3 fixture")
}

/// WGS84 geodetic (degrees, metres) to ECEF metres, as the tests' helper forms it.
fn geodetic_to_ecef(lat_deg: f64, lon_deg: f64, h_m: f64) -> [f64; 3] {
    let a = 6_378_137.0;
    let f = 1.0 / 298.257_223_563;
    let e2 = f * (2.0 - f);
    let lat = lat_deg * std::f64::consts::PI / 180.0;
    let lon = lon_deg * std::f64::consts::PI / 180.0;
    let n = a / (1.0 - e2 * lat.sin() * lat.sin()).sqrt();
    [
        (n + h_m) * lat.cos() * lon.cos(),
        (n + h_m) * lat.cos() * lon.sin(),
        (n * (1.0 - e2) + h_m) * lat.sin(),
    ]
}

fn norm3(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Synthetic code pseudoranges from an SP3 product: the Earth-rotated light-time
/// range to each satellite of `systems` above `min_elevation_deg`, plus
/// `c (dtr - dts)` with `dts` the product clock carrying the RTKLIB `peph2pos`
/// relativistic term `-2 r.v / c^2` (the velocity the difference of the product
/// positions 1 ms apart), which the positioning model applies to a precise clock.
fn synth_sp3(
    sp3: &Sp3,
    t_rx: f64,
    rx: [f64; 3],
    rx_clock_s: f64,
    min_elevation_deg: f64,
    systems: &[GnssSystem],
) -> Vec<(GnssSatelliteId, f64)> {
    let rx_radius = norm3(rx);
    let up = [rx[0] / rx_radius, rx[1] / rx_radius, rx[2] / rx_radius];
    let mut out = Vec::new();
    for sat in sp3.satellites().iter().copied() {
        if !systems.contains(&sat.system) {
            continue;
        }
        let mut dt_flight = 0.075;
        let mut placed = None;
        for _ in 0..4 {
            let t_tx = t_rx - dt_flight;
            let Ok(state) = sp3.position_at_j2000_seconds(sat, t_tx) else {
                placed = None;
                break;
            };
            let Some(dt_sat) = state.clock_s else {
                placed = None;
                break;
            };
            let raw = state.position.as_array();
            if !raw[0].is_finite() || !dt_sat.is_finite() {
                placed = None;
                break;
            }
            let theta = OMEGA_E * dt_flight;
            let p = [
                raw[0] * theta.cos() + raw[1] * theta.sin(),
                -raw[0] * theta.sin() + raw[1] * theta.cos(),
                raw[2],
            ];
            let range = norm3([p[0] - rx[0], p[1] - rx[1], p[2] - rx[2]]);
            placed = Some((t_tx, raw, p, range, dt_sat));
            dt_flight = range / C_M_S;
        }
        let Some((t_tx, raw, p, range, dt_sat)) = placed else {
            continue;
        };
        let Ok(later) = sp3.position_at_j2000_seconds(sat, t_tx + 1.0e-3) else {
            continue;
        };
        let raw_later = later.position.as_array();
        let v = [
            (raw_later[0] - raw[0]) / 1.0e-3,
            (raw_later[1] - raw[1]) / 1.0e-3,
            (raw_later[2] - raw[2]) / 1.0e-3,
        ];
        let relativity_s = -2.0 * (raw[0] * v[0] + raw[1] * v[1] + raw[2] * v[2]) / C_M_S / C_M_S;
        let los = [p[0] - rx[0], p[1] - rx[1], p[2] - rx[2]];
        let el_deg = ((los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range)
            .asin()
            .to_degrees();
        if el_deg < min_elevation_deg {
            continue;
        }
        out.push((sat, range + C_M_S * (rx_clock_s - (dt_sat + relativity_s))));
    }
    out
}

fn observations_json(observations: &[(GnssSatelliteId, f64)]) -> Value {
    Value::Array(
        observations
            .iter()
            .map(|(sat, pr)| json!({ "satelliteId": sat.to_string(), "pseudorangeM": hex(*pr) }))
            .collect(),
    )
}

/// The core `SolveInputs` the binding builds from an SPP request that sets only
/// the given fields: no corrections, zero Klobuchar coefficients, default
/// surface meteorology, no GLONASS channels, no robust loop, single-frequency code.
fn solve_inputs(
    observations: &[(GnssSatelliteId, f64)],
    t_rx: f64,
    second_of_day: f64,
    day_of_year: f64,
    initial_guess: [f64; 4],
) -> SolveInputs {
    SolveInputs {
        observations: observations
            .iter()
            .map(|(sat, pr)| Observation {
                satellite_id: *sat,
                pseudorange_m: *pr,
            })
            .collect(),
        t_rx_j2000_s: t_rx,
        t_rx_second_of_day_s: second_of_day,
        day_of_year,
        initial_guess,
        corrections: Corrections::NONE,
        klobuchar: KlobucharCoeffs {
            alpha: [0.0; 4],
            beta: [0.0; 4],
        },
        beidou_klobuchar: None,
        galileo_nequick: None,
        sbas_iono: None,
        glonass_channels: BTreeMap::new(),
        met: SurfaceMet::default(),
        troposphere_model: TroposphereModel::Rtklib,
        robust: None,
        pseudorange_code: PseudorangeCode::SingleFrequency,
        qzss_clock: QzssClock::Gps,
    }
}

fn default_policy() -> SolvePolicy {
    SolvePolicy {
        validation: SolutionValidationOptions::default(),
        coarse_search_seeds: None,
    }
}

/// The velocity golden scenario of sidereon-core's velocity module test: range
/// rates predicted for a receiver moving at (12, -7, 3) m/s with a 1 ns/s clock
/// drift over the SP3 product, and the range-rate and Doppler solutions.
fn velocity_section() -> (Value, Vec<(GnssSatelliteId, f64)>) {
    const T_RX: f64 = 646_272_000.0;
    const RECEIVER: [f64; 3] = [4_500_000.0, 500_000.0, 4_500_000.0];
    const V_TRUE: [f64; 3] = [12.0, -7.0, 3.0];
    const DRIFT_TRUE: f64 = 1.0e-9;
    let sp3 = load_sp3(GRG_SP3);
    let mut planning = PredictOptions::default();
    planning.light_time = false;
    let sats: Vec<GnssSatelliteId> = sp3
        .satellites()
        .iter()
        .copied()
        .filter(|sat| sat.system == GnssSystem::Gps)
        .filter(|sat| {
            predict(&sp3, *sat, RECEIVER, T_RX, planning)
                .map(|obs| obs.elevation_deg >= 5.0)
                .unwrap_or(false)
        })
        .collect();
    let range_rates: Vec<(GnssSatelliteId, f64)> = sats
        .iter()
        .map(|&sat| {
            let obs = predict(&sp3, sat, RECEIVER, T_RX, PredictOptions::default())
                .expect("predict synthetic observation");
            let e_dot = obs.los_unit[0] * V_TRUE[0]
                + obs.los_unit[1] * V_TRUE[1]
                + obs.los_unit[2] * V_TRUE[2];
            (sat, obs.range_rate_m_s - e_dot + C_M_S * DRIFT_TRUE)
        })
        .collect();
    let rr_obs: Vec<VelocityObservation> = range_rates
        .iter()
        .map(|&(sat, value)| VelocityObservation {
            satellite_id: sat,
            value,
            carrier_hz: F_L1_HZ,
            sat_clock_drift_s_s: 0.0,
        })
        .collect();
    let rr = solve_velocity(
        &sp3,
        &rr_obs,
        RECEIVER,
        T_RX,
        VelocitySolveOptions::default(),
    )
    .expect("range-rate solve");
    let dop_obs: Vec<VelocityObservation> = rr_obs
        .iter()
        .enumerate()
        .map(|(idx, obs)| {
            let channel = (idx % 14) as i8 - 7;
            let carrier_hz = sidereon_core::frequencies::rinex_band_frequency_hz(
                GnssSystem::Glonass,
                '1',
                Some(channel),
            )
            .expect("GLONASS G1 channel carrier");
            VelocityObservation {
                satellite_id: obs.satellite_id,
                value: range_rate_to_doppler(obs.value, carrier_hz).expect("rr to doppler"),
                carrier_hz,
                sat_clock_drift_s_s: 0.0,
            }
        })
        .collect();
    let mut dop_options = VelocitySolveOptions::default();
    dop_options.observable = VelocityObservable::Doppler;
    let dop = solve_velocity(&sp3, &dop_obs, RECEIVER, T_RX, dop_options).expect("doppler solve");
    let section = json!({
        "tRxJ2000S": hex(T_RX),
        "receiverEcefM": hexes(&RECEIVER),
        "rangeRates": range_rates
            .iter()
            .map(|(sat, value)| json!([sat.to_string(), hex(*value)]))
            .collect::<Vec<_>>(),
        "rangeRate": {
            "usedSats": rr.used_sats.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "velocityMS": hexes(&rr.velocity_m_s),
            "stateCovariance": mat4(&rr.state_covariance),
            "speedMS": hex(rr.speed_m_s),
            "clockDriftSS": hex(rr.clock_drift_s_s),
            "residualsMS": rr.residuals_m_s.iter().map(|(_, r)| hex(*r)).collect::<Vec<_>>(),
        },
        "doppler": {
            "values": dop_obs.iter().map(|o| hex(o.value)).collect::<Vec<_>>(),
            "carriersHz": dop_obs.iter().map(|o| hex(o.carrier_hz)).collect::<Vec<_>>(),
            "velocityMS": hexes(&dop.velocity_m_s),
            "stateCovariance": mat4(&dop.state_covariance),
            "speedMS": hex(dop.speed_m_s),
            "clockDriftSS": hex(dop.clock_drift_s_s),
        },
    });
    (section, range_rates)
}

/// `solveSppWithDopplerVelocity` over synthetic pseudoranges and the velocity
/// scenario's range rates converted to L1 Doppler.
fn spp_doppler_section(range_rates: &[(GnssSatelliteId, f64)]) -> Value {
    let sp3 = load_sp3(GRG_SP3);
    let receiver = [4_500_000.0, 500_000.0, 4_500_000.0];
    let t_rx = 646_272_000.0;
    let observations = synth_sp3(&sp3, t_rx, receiver, 0.0, 10.0, &[GnssSystem::Gps]);
    let inputs = solve_inputs(
        &observations,
        t_rx,
        43200.0,
        176.0,
        [receiver[0], receiver[1], receiver[2], 0.0],
    );
    let doppler: Vec<DopplerObservation> = range_rates
        .iter()
        .map(|&(sat, rr)| DopplerObservation {
            satellite_id: sat,
            doppler_hz: -(rr * F_L1_HZ) / C_M_S,
            carrier_hz: F_L1_HZ,
            sat_clock_drift_s_s: 0.0,
        })
        .collect();
    let fused = solve_with_doppler_velocity(&sp3, &inputs, &doppler, true).expect("fused solve");
    let velocity = fused.velocity.expect("velocity solved");
    json!({
        "tRxJ2000S": hex(t_rx),
        "receiverEcefM": hexes(&receiver),
        "observations": observations_json(&observations),
        "doppler": doppler
            .iter()
            .map(|d| json!({ "satelliteId": d.satellite_id.to_string(), "dopplerHz": hex(d.doppler_hz), "carrierHz": hex(d.carrier_hz) }))
            .collect::<Vec<_>>(),
        "receiver": {
            "rxClockDriftSS": hex(fused.receiver.rx_clock_drift_s_s.expect("fused clock drift")),
            "positionM": hexes(&fused.receiver.position.as_array()),
            "positionCovarianceEcefM2": mat3(&fused.receiver.position_covariance.ecef_m2),
        },
        "velocity": {
            "velocityMS": hexes(&velocity.velocity_m_s),
            "speedMS": hex(velocity.speed_m_s),
            "clockDriftSS": hex(velocity.clock_drift_s_s),
            "stateCovariance": mat4(&velocity.state_covariance),
        },
    })
}

/// The static positioning test's four-epoch solve at a mid-latitude receiver.
fn static_section() -> Value {
    let sp3 = load_sp3(GRG_SP3);
    let rx = geodetic_to_ecef(48.0, 11.0, 600.0);
    let epochs = sp3.epochs_j2000_seconds();
    let indices = [40usize, 44, 48, 52];
    let per_epoch: Vec<(f64, Vec<(GnssSatelliteId, f64)>)> = indices
        .iter()
        .map(|&index| {
            let t = epochs[index];
            (t, synth_sp3(&sp3, t, rx, 0.0, 10.0, &[GnssSystem::Gps]))
        })
        .collect();
    let static_epochs: Vec<StaticEpoch> = per_epoch
        .iter()
        .map(|(t, observations)| {
            StaticEpoch::from_solve_inputs(solve_inputs(
                observations,
                *t,
                43200.0,
                176.0,
                [rx[0], rx[1], rx[2], 0.0],
            ))
        })
        .collect();
    let mut options = StaticSolveOptions::default();
    options.initial_position_m = rx;
    options.with_geodetic = true;
    options.robust = None;
    let solution = solve_static(&sp3, &static_epochs, options).expect("static solve");
    let geodetic = solution.geodetic.expect("geodetic requested");
    json!({
        "receiverEcefM": hexes(&rx),
        "epochs": per_epoch
            .iter()
            .map(|(t, observations)| json!({ "tRxJ2000S": hex(*t), "observations": observations_json(observations) }))
            .collect::<Vec<_>>(),
        "positionM": hexes(&solution.position.as_array()),
        "geodetic": hexes(&[geodetic.lat_rad, geodetic.lon_rad, geodetic.height_m]),
        "residualRmsM": hex(solution.residual_rms_m()),
        "stateParameterCount": solution.covariance.state_m2.len(),
        "positionCovarianceEcefM2": mat3(&solution.covariance.position_ecef_m2),
        "positionCovarianceEnuM2": mat3(&solution.covariance.position_enu_m2),
        "usedSatCounts": solution.used_sats.iter().map(Vec::len).collect::<Vec<_>>(),
        "residualCount": solution.residuals_m.len(),
        "perEpochClockCount": solution.per_epoch_clock.len(),
        "metadata": {
            "iterations": solution.metadata.iterations,
            "converged": solution.metadata.converged,
            "status": format!("{:?}", solution.metadata.status),
            "outerIterations": solution.metadata.outer_iterations,
            "finalRobustScaleM": solution.metadata.final_robust_scale_m,
            "usedMeasurements": solution.metadata.used_measurements,
            "nParameters": solution.metadata.n_parameters,
            "redundancy": solution.metadata.redundancy,
        },
    })
}

/// The SPP solve over the committed L0 trace inputs (no ionosphere, no
/// troposphere). The trace's own solution leaves out the precise-clock
/// relativistic term, which positioning applies, so the engine's solve with
/// the term is the reference for the binding.
fn spp_trace_section() -> Value {
    let doc: Value =
        serde_json::from_str(&fixture_text("spp_trace_L0_minimal.json")).expect("trace JSON");
    let fx = &doc["fixture"];
    let inp = &fx["inputs"];
    let sp3 = load_sp3(inp["sp3_file"].as_str().expect("sp3 file"));
    let observations: Vec<(GnssSatelliteId, f64)> = inp["observations"]
        .as_array()
        .expect("observations")
        .iter()
        .map(|o| {
            (
                GnssSatelliteId::from_str(o["sat_id"].as_str().expect("sat")).expect("token"),
                from_hex(o["p_meas_m"].as_str().expect("p")),
            )
        })
        .collect();
    let x0: Vec<f64> = fx["frozen"]["initial_guess_x0"]
        .as_array()
        .expect("x0")
        .iter()
        .map(|v| from_hex(v.as_str().expect("hex")))
        .collect();
    let four = |key: &str| -> [f64; 4] {
        let v: Vec<f64> = inp[key]
            .as_array()
            .expect("coefficients")
            .iter()
            .map(|v| from_hex(v.as_str().expect("hex")))
            .collect();
        [v[0], v[1], v[2], v[3]]
    };
    let mut inputs = solve_inputs(
        &observations,
        from_hex(inp["t_rx_j2000_s"].as_str().expect("t")),
        from_hex(inp["t_rx_sod_s"].as_str().expect("sod")),
        from_hex(inp["doy"].as_str().expect("doy")),
        [x0[0], x0[1], x0[2], x0[3]],
    );
    inputs.klobuchar = KlobucharCoeffs {
        alpha: four("klobuchar_alpha"),
        beta: four("klobuchar_beta"),
    };
    inputs.met = SurfaceMet {
        pressure_hpa: from_hex(inp["met"]["pressure_hpa"].as_str().expect("p")),
        temperature_k: from_hex(inp["met"]["temperature_k"].as_str().expect("t")),
        relative_humidity: from_hex(inp["met"]["relative_humidity"].as_str().expect("h")),
    };
    let solution = sidereon::solve_spp(
        &sp3 as &dyn EphemerisSource,
        &inputs,
        true,
        default_policy(),
    )
    .expect("trace SPP solve");
    let m = &solution.metadata;
    json!({
        "positionM": hexes(&solution.position.as_array()),
        "rxClockS": hex(solution.rx_clock_s),
        "metadata": {
            "iterations": m.iterations,
            "converged": m.converged,
            "status": format!("{:?}", m.status),
            "outerIterations": m.outer_iterations,
            "finalRobustScaleM": m.final_robust_scale_m,
            "ionosphereApplied": m.ionosphere_applied,
            "troposphereApplied": m.troposphere_applied,
            "usedCount": m.used_count,
            "systems": m.systems.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        },
    })
}

/// `raimForSolution` over an SPP solve of geometric ranges (no light time, no
/// relativistic term) at a real receiver, as the RAIM test synthesizes them.
fn raim_solution_section() -> Value {
    let sp3 = load_sp3(GBM_SP3);
    let t_rx = sp3.epochs_j2000_seconds()[12];
    let rx = [3_582_105.291, 532_589.7313, 5_232_754.8054];
    let observations: Vec<(GnssSatelliteId, f64)> = ["G05", "G07", "G08", "G10", "G13", "G15"]
        .iter()
        .map(|token| {
            let sat = GnssSatelliteId::from_str(token).expect("token");
            let state = sp3.position_at_j2000_seconds(sat, t_rx).expect("state");
            let p = state.position.as_array();
            let range = norm3([p[0] - rx[0], p[1] - rx[1], p[2] - rx[2]]);
            (sat, range - C_M_S * state.clock_s.expect("clock"))
        })
        .collect();
    let inputs = solve_inputs(
        &observations,
        t_rx,
        3600.0,
        177.0,
        [rx[0], rx[1], rx[2], 0.0],
    );
    let solution = sidereon::solve_spp(
        &sp3 as &dyn EphemerisSource,
        &inputs,
        true,
        default_policy(),
    )
    .expect("RAIM SPP solve");
    let mut options = RaimOptions::default();
    options.p_fa = 1e-3;
    let result = raim_for_solution(&solution, &options).expect("RAIM");
    json!({
        "tRxJ2000S": hex(t_rx),
        "observations": observations_json(&observations),
        "usedSatCount": solution.used_sats.len(),
        "faultDetected": result.fault_detected,
        "dof": result.dof,
        "worstSat": result.worst_sat,
        "testStatistic": hex(result.test_statistic),
    })
}

/// The broadcast store's state for its first record at that record's `toe`.
fn nav_store_section() -> Value {
    let text = fixture_text("nav/BRD400DLR_S_20261800000_01H_MN_trim.rnx");
    let store = BroadcastEphemeris::from_nav(&text).expect("broadcast store");
    let record = store.records()[0].clone();
    let gps_epoch = j2000_seconds(1980, 1, 6, 0, 0, 0.0);
    let query = gps_epoch + f64::from(record.week) * 604_800.0 + record.elements.toe_sow;
    let (position, clock) = store
        .position_clock_at_j2000_s(record.satellite_id, query)
        .expect("store state");
    json!({
        "satellite": record.satellite_id.to_string(),
        "tJ2000S": hex(query),
        "clockS": hex(clock),
        "positionM": hexes(&position),
    })
}

/// The repaired NAV text of the BRD4 trim file with unsupported blocks dropped
/// and records sorted.
fn nav_repair_section() -> Value {
    let text = fixture_text("nav/BRD400DLR_S_20261800000_01H_MN_trim.rnx");
    let mut options = RepairOptions::default();
    options.drop_unsupported = true;
    options.sort_records = true;
    let repair = sidereon::repair_rinex_nav(&text, &options).expect("repair");
    let repaired = sidereon_core::rinex::nav::encode_nav(&repair.records).expect("encode");
    json!({
        "recordCount": repair.records.len(),
        "repairedTextLength": repaired.len(),
    })
}

/// Record and skip counts of the Bias-SINEX and CODE DCB fixtures.
fn bias_section() -> Value {
    let sinex = BiasSet::parse_bias_sinex(&fixture_bytes("bias/CODE.BIA")).expect("Bias-SINEX");
    let dcb = BiasSet::parse_code_dcb(&fixture_bytes("bias/P1C1_RINEX.DCB"), None).expect("DCB");
    json!({
        "sinexRecordCount": sinex.value.records().len(),
        "sinexSkippedRecords": sinex.value.skipped_records(),
        "dcbRecordCount": dcb.value.records().len(),
        "dcbSkippedRecords": dcb.value.skipped_records(),
    })
}

/// The scenario simulator test's two-epoch synthetic Keplerian scenario.
fn scenario_section() -> Value {
    let start = 820_497_600.0_f64;
    let pi = std::f64::consts::PI;
    let satellites: Vec<Value> = [
        (1, 0.0, 0.0, 0.0),
        (2, 0.0, 0.0, pi / 3.0),
        (3, 0.0, 0.0, -pi / 3.0),
        (4, 0.0, pi / 2.0, pi / 3.0),
        (5, 0.0, pi / 2.0, -pi / 3.0),
    ]
    .iter()
    .map(|&(prn, raan, inclination, mean_anomaly)| {
        json!({
            "satellite_id": { "system": "Gps", "prn": prn },
            "semi_major_axis_m": 26_560_000.0,
            "eccentricity": 0.0,
            "inclination_rad": inclination,
            "raan_rad": raan,
            "arg_perigee_rad": 0.0,
            "mean_anomaly_rad": mean_anomaly,
            "epoch_j2000_s": start,
            "clock_bias_s": 0.0,
            "clock_drift_s_s": 0.0,
        })
    })
    .collect();
    let scenario = json!({
        "schema_version": 1,
        "seed": 123_456_789u64,
        "epochs": { "start_j2000_s": start, "count": 2, "cadence_s": 30.0 },
        "receiver": { "kind": "static_geodetic", "position": { "lat_rad": 0.0, "lon_rad": 0.0, "height_m": 0.0 } },
        "constellation": { "kind": "synthetic_keplerian", "satellites": satellites },
        "signals": [{
            "system": "Gps",
            "code_observable": "C1C",
            "phase_observable": "L1C",
            "doppler_observable": "D1C",
            "carrier_hz": 1_575_420_000.0,
            "carrier_phase_bias_cycles": 12.25,
        }],
        "error_budget": {
            "receiver_clock": {
                "enabled": true,
                "bias_s": 1e-7,
                "drift_s_s": 1e-10,
                "power_law_coefficients": [1e-24, 1e-26, 1e-22, 1e-26, 1e-28],
            },
            "satellite_clock": {
                "enabled": false,
                "bias_s": 0.0,
                "drift_s_s": 0.0,
                "power_law_coefficients": [0.0, 0.0, 0.0, 0.0, 0.0],
            },
            "ionosphere": { "kind": "off" },
            "troposphere": { "kind": "off" },
            "thermal_noise": {
                "enabled": true,
                "pseudorange_sigma_m": 0.25,
                "carrier_phase_sigma_m": 0.002,
                "doppler_sigma_hz": 0.02,
            },
            "multipath": { "enabled": true, "amplitude_m": 0.15, "reflector_height_m": 1.25, "phase_rad": 0.3 },
            "elevation_mask_deg": -90.0,
        },
    });
    let scenario: sidereon_core::scenario::Scenario =
        serde_json::from_value(scenario).expect("scenario schema");
    let set = sidereon_core::scenario::simulate_scenario(&scenario).expect("simulate");
    json!({
        "observationCount": set.observations.satellite_id.len(),
        "epochOffsets": set.observations.epoch_offsets,
        "firstSatellite": set.observations.satellite_id[0].to_string(),
        "pseudorangeM0": hex(set.observations.pseudorange_m[0]),
        "carrierPhaseCycles0": hex(set.observations.carrier_phase_cycles[0]),
        "dopplerHz0": hex(set.observations.doppler_hz[0]),
        "geometricRangeM0": hex(set.truth_terms.geometric_range_m[0]),
        "thermalNoiseM0": hex(set.truth_terms.thermal_noise_m[0]),
        "receiverTruth1PositionEcefM0": hex(set.receiver_truth[1].position_ecef_m[0]),
        "determinismFingerprintHex": format!("0x{:016x}", set.determinism_fingerprint()),
        "rinexText": set.to_rinex_string().expect("scenario RINEX"),
        "sppObservationsEpoch0": set
            .spp_observations_for_epoch(0)
            .iter()
            .map(|obs| json!({
                "satelliteId": obs.satellite_id.to_string(),
                "pseudorangeM": hex(obs.pseudorange_m),
            }))
            .collect::<Vec<_>>(),
    })
}

/// The fusion test's tight SP3 update of a fifteen-state EKF with one G08 code
/// observation.
fn fusion_tight_section() -> Value {
    let sp3 = load_sp3(GRG_SP3);
    let nominal = core_inertial::NavState::new(
        646_272_000.0,
        [4_484_128.0, 550_582.0, 4_487_561.0],
        [0.0, 0.0, 0.0],
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    )
    .expect("nav state")
    .with_biases([0.0; 3], [0.0; 3])
    .expect("biases");
    let state = core_fusion::InsFilterState::from_diagonal(
        nominal,
        core_fusion::ErrorStateLayout::Fifteen,
        &[10.0; 15],
    )
    .expect("filter state");
    let mut config = core_fusion::InertialFilterConfig::new(core_inertial::ImuSpec::preset(
        core_inertial::ImuGrade::Mems,
    ))
    .expect("filter config");
    config.filter_kind = core_fusion::FusionFilterKind::Ekf;
    let mut tight = core_fusion::TightCouplingConfig::default();
    tight.light_time = true;
    tight.sagnac = true;
    tight.initial_clock_bias_variance_m2 = 1e8;
    tight.initial_clock_drift_variance_m2_s2 = 1e4;
    tight.update_options = core_fusion::EkfUpdateOptions::default();
    tight.update_options.innovation_gate = None;
    config.tight = tight;
    let mut filter = core_fusion::InertialFilter::with_config(state, config).expect("filter");
    let observation = core_fusion::TightGnssObservation {
        satellite_id: GnssSatelliteId::from_str("G08").expect("token"),
        pseudorange_m: 23_825_519.8,
        pseudorange_sigma_m: 3.0,
        range_rate: None,
        carrier_phase: None,
        ionosphere_delay_m: 0.0,
        troposphere_delay_m: 0.0,
    };
    observation.validate().expect("observation");
    let epoch = core_fusion::TightGnssEpoch::new(646_272_000.0, vec![observation]).expect("epoch");
    let update = filter.update_tight(&sp3, &epoch).expect("tight update");
    let clock = filter.tight_clock_state().expect("clock state");
    json!({
        "applied": update.applied,
        "rows": update.rows,
        "nis": hex(update.nis),
        "clockBiasM": hex(clock.bias_m),
        "clockCovariance0": hex(clock.covariance[0][0]),
    })
}

fn ppp_epochs(fx: &Value) -> Vec<FloatEpoch> {
    fx["epochs"]
        .as_array()
        .expect("epochs")
        .iter()
        .map(|epoch| {
            let civil = &epoch["civil"];
            FloatEpoch {
                epoch: CivilDateTime {
                    year: civil["year"].as_i64().expect("year") as i32,
                    month: civil["month"].as_u64().expect("month") as u8,
                    day: civil["day"].as_u64().expect("day") as u8,
                    hour: civil["hour"].as_u64().expect("hour") as u8,
                    minute: civil["minute"].as_u64().expect("minute") as u8,
                    second: civil["second"].as_f64().expect("second"),
                },
                jd_whole: epoch["jd_whole"].as_f64().expect("jd_whole"),
                jd_fraction: epoch["jd_fraction"].as_f64().expect("jd_fraction"),
                t_rx_j2000_s: epoch["t_rx_j2000_s"].as_f64().expect("t_rx"),
                observations: epoch["observations"]
                    .as_array()
                    .expect("observations")
                    .iter()
                    .map(|obs| {
                        let token = obs["satellite_id"].as_str().expect("satellite");
                        FloatObservation {
                            sat: GnssSatelliteId::from_str(token).expect("token"),
                            satellite_id: token.to_string(),
                            ambiguity_id: obs["ambiguity_id"].as_str().expect("amb").to_string(),
                            code_m: obs["code_m"].as_f64().expect("code"),
                            phase_m: obs["phase_m"].as_f64().expect("phase"),
                            freq1_hz: obs["freq1_hz"].as_f64().expect("f1"),
                            freq2_hz: obs["freq2_hz"].as_f64().expect("f2"),
                            glonass_channel: None,
                            signals: None,
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

fn ppp_state(fx: &Value) -> FloatState {
    let st = &fx["initial_state"];
    let f = |v: &Value| v.as_f64().expect("number");
    let pos: Vec<f64> = st["position_m"]
        .as_array()
        .expect("pos")
        .iter()
        .map(f)
        .collect();
    FloatState {
        position_m: [pos[0], pos[1], pos[2]],
        clocks_m: st["clocks_m"]
            .as_array()
            .expect("clocks")
            .iter()
            .map(f)
            .collect(),
        // The fixture writes the ambiguity map as `[id, metres]` pairs; an
        // object form is read the same way.
        ambiguities_m: match &st["ambiguities_m"] {
            Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), f(v))).collect(),
            Value::Array(pairs) => pairs
                .iter()
                .map(|pair| (pair[0].as_str().expect("id").to_string(), f(&pair[1])))
                .collect(),
            other => panic!("ambiguities_m: {other}"),
        },
        ztd_m: st.get("ztd_m").and_then(Value::as_f64).unwrap_or(0.0),
        tropo_gradient_north_m: st
            .get("tropo_gradient_north_m")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        tropo_gradient_east_m: st
            .get("tropo_gradient_east_m")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        residual_ionosphere_m: BTreeMap::new(),
    }
}

fn ppp_weights(raw: &Value) -> MeasurementWeights {
    MeasurementWeights {
        code: raw["code"].as_f64().expect("code"),
        phase: raw["phase"].as_f64().expect("phase"),
        elevation_weighting: raw["elevation_weighting"].as_bool().expect("ew"),
    }
}

fn ppp_tropo(raw: &Value, gradients: bool) -> TroposphereOptions {
    if !raw["enabled"].as_bool().expect("enabled") {
        return TroposphereOptions::disabled();
    }
    let met = Met::new(
        raw["pressure_hpa"].as_f64().expect("p"),
        raw["temperature_k"].as_f64().expect("t"),
        raw["relative_humidity"].as_f64().expect("h"),
    )
    .expect("met");
    let mut opts = TroposphereOptions::new(met);
    opts.enabled = true;
    opts.estimate_ztd = raw["estimate_ztd"].as_bool().expect("ztd");
    opts.estimate_tropo_gradients = gradients
        || raw
            .get("estimate_tropo_gradients")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    opts.mapping = TropoMapping::Niell;
    opts
}

fn ppp_options(raw: &Value) -> FloatSolveOptions {
    let mut options = FloatSolveOptions::default();
    options.max_iterations = raw["max_iterations"].as_u64().expect("iterations") as usize;
    options.position_tolerance_m = raw["position_tolerance_m"].as_f64().expect("pos tol");
    options.clock_tolerance_m = raw["clock_tolerance_m"].as_f64().expect("clock tol");
    options.ambiguity_tolerance_m = raw["ambiguity_tolerance_m"].as_f64().expect("amb tol");
    options.ztd_tolerance_m = raw["ztd_tolerance_m"].as_f64().expect("ztd tol");
    options
}

fn ppp_float_config(fx: &Value, gradients: bool, cutoff: Option<f64>) -> FloatSolveConfig {
    let cfg = &fx["config"];
    FloatSolveConfig::new(
        ppp_weights(&cfg["weights"]),
        ppp_tropo(&cfg["tropo"], gradients),
        RangeCorrections::disabled(),
        ppp_options(&cfg["opts"]),
        cutoff.or_else(|| cfg.get("elevation_cutoff_deg").and_then(Value::as_f64)),
        cfg["residual_screen"].as_bool().expect("screen"),
        cfg.get("estimate_residual_ionosphere")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}

fn ppp_fixed_config(fx: &Value) -> FixedSolveConfig {
    let cfg = &fx["fixed_config"];
    let amb = &cfg["ambiguity"];
    let map = |v: &Value| -> BTreeMap<String, f64> {
        v.as_object()
            .expect("map")
            .iter()
            .map(|(k, v)| (k.clone(), v.as_f64().expect("number")))
            .collect()
    };
    let mut ambiguity = FixedAmbiguityOptions::new(amb["ratio_threshold"].as_f64().expect("ratio"));
    ambiguity.wavelengths_m = map(&amb["wavelengths_m"]);
    ambiguity.offsets_m = map(&amb["offsets_m"]);
    FixedSolveConfig::new(
        ppp_weights(&cfg["weights"]),
        ppp_tropo(&cfg["tropo"], false),
        RangeCorrections::disabled(),
        ppp_options(&cfg["opts"]),
        cfg.get("elevation_cutoff_deg").and_then(Value::as_f64),
        ambiguity,
        cfg.get("estimate_residual_ionosphere")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}

fn temporal_json(t: &sidereon_core::precise_positioning::TemporalCorrelationSummary) -> Value {
    json!({
        "lag1Autocorrelation": hex(t.lag1_autocorrelation),
        "decorrelationTimeEpochs": hex(t.decorrelation_time_epochs),
        "decorrelationTimeS": t.decorrelation_time_s.map(hex),
        "effectiveSampleCount": hex(t.effective_sample_count),
        "varianceInflationFactor": hex(t.variance_inflation_factor),
        "nominalSampleCount": t.nominal_sample_count,
        "arcsUsed": t.arcs_used,
    })
}

fn mat2(matrix: &[[f64; 2]; 2]) -> Vec<String> {
    matrix.iter().flatten().copied().map(hex).collect()
}

/// The PPP float, fixed, elevation-cutoff and troposphere-gradient solves of
/// the ESBC arc (`ppp_esbc.json`) with the configurations the PPP tests map.
fn ppp_section() -> Value {
    let fx: Value = serde_json::from_str(&fixture_text("ppp_esbc.json")).expect("ppp fixture");
    let sp3 = load_sp3(&format!(
        "sp3/{}",
        fx["sp3_file"].as_str().expect("sp3 file")
    ));
    let epochs = ppp_epochs(&fx);
    let float = sidereon::solve_ppp_float(
        &sp3,
        &epochs,
        ppp_state(&fx),
        ppp_float_config(&fx, false, None),
    )
    .expect("float solve");
    let cutoff = sidereon::solve_ppp_float(
        &sp3,
        &epochs,
        ppp_state(&fx),
        ppp_float_config(&fx, false, Some(30.0)),
    )
    .expect("cutoff solve");
    let gradients = sidereon::solve_ppp_float(
        &sp3,
        &epochs,
        ppp_state(&fx),
        ppp_float_config(&fx, true, None),
    )
    .expect("gradient solve");
    let fixed = sidereon::solve_ppp_fixed(&sp3, &epochs, float.clone(), ppp_fixed_config(&fx))
        .expect("fixed solve");
    json!({
        "float": {
            "status": format!("{:?}", float.status),
            "solvedEpochCount": float.solved_epoch_indices.len(),
            "epochClockCount": float.epoch_clocks_m.len(),
            "residualCount": float.residuals_m.len(),
            "positionCovarianceEcefM2": mat3(&float.position_covariance.ecef_m2),
            "temporalPositionCovarianceEcefM2": mat3(&float.temporal_position_covariance.ecef_m2),
            "temporalCorrelation": temporal_json(&float.temporal_correlation),
        },
        "cutoff30": {
            "usedSatCount": cutoff.used_sats.len(),
        },
        "gradients": {
            "northM": hex(gradients.tropo_gradient_north_m.expect("north")),
            "eastM": hex(gradients.tropo_gradient_east_m.expect("east")),
            "covarianceM2": mat2(gradients.tropo_gradient_covariance_m2.as_ref().expect("cov")),
            "formalCovarianceM2": mat2(gradients.formal_tropo_gradient_covariance_m2.as_ref().expect("formal")),
        },
        "fixed": {
            "status": format!("{:?}", fixed.status),
            "residualCount": fixed.residuals_m.len(),
            "nominalSampleCount": fixed.temporal_correlation.nominal_sample_count,
        },
    })
}

/// The real WTZR/WTZZ static reference-station carrier solve of the RTK arc
/// test (24 epochs, split-arc cycle slips, partial ambiguity resolution).
fn rtk_static_reference_section() -> Value {
    use sidereon_core::positioning::{
        solve_static_reference_station_rinex, StaticReferenceCarrierRinexOptions,
        StaticReferenceStationRinexOptions,
    };
    use sidereon_core::rinex::observations::RinexObs;
    use sidereon_core::rtk::{BaselineReferenceSelection, CycleSlipPolicy};
    use sidereon_core::rtk_filter::{
        defaults, DynamicsModel, FixedSolveOpts, FloatSolveOpts, MeasModel, ResidualValidationOpts,
        RtkArcConfig, RtkArcPreprocessing, RtkRinexArcOptions, RtkStaticArcConfig, SearchOpts,
        StochasticModel, UpdateOpts, ValidatedFixedSolveOpts,
    };

    let sp3 = load_sp3("sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3");
    let base_obs = RinexObs::parse(&fixture_text(
        "obs/WTZR00DEU_R_20201770000_01D_30S_MO_120epoch.rnx",
    ))
    .expect("base OBS");
    let rover_obs = RinexObs::parse(&fixture_text(
        "obs/WTZZ00DEU_R_20201770000_01D_30S_MO_120epoch.rnx",
    ))
    .expect("rover OBS");
    let marker = [4_075_580.3111, 931_854.0543, 4_801_568.2808];
    let delta = base_obs.header.antenna_delta_hen_m.expect("antenna delta");
    assert_eq!((delta[1], delta[2]), (0.0, 0.0));
    let radius = norm3(marker);
    let reference = [
        marker[0] + marker[0] / radius * delta[0],
        marker[1] + marker[1] / radius * delta[0],
        marker[2] + marker[2] / radius * delta[0],
    ];

    let defaults_arc = RtkRinexArcOptions::gps_l1_c();
    let arc_options = RtkRinexArcOptions::new(
        defaults_arc.signal_pairs,
        Some(24),
        defaults_arc.min_common_satellites,
        false,
    );
    let model = MeasModel {
        code_sigma_m: 2.0,
        phase_sigma_m: 0.01,
        sagnac: true,
        stochastic: StochasticModel::Simple {
            elevation_weighting: true,
        },
    };
    let update = UpdateOpts {
        hold_sigma_m: 1.0e-4,
        position_tol_m: defaults::POSITION_TOL_M,
        ambiguity_tol_m: defaults::AMBIGUITY_TOL_M,
        max_iterations: defaults::MAX_ITERATIONS,
        process_noise_baseline_sigma_m: 0.0,
        dynamics_model: DynamicsModel::ConstantPosition,
        float_only_systems: Vec::new(),
        report_residuals: false,
        receiver_antenna_corrections: None,
        ar_arming_sigma_m: None,
        search: SearchOpts {
            ratio_threshold: defaults::RATIO_THRESHOLD,
        },
    };
    let preprocessing = RtkArcPreprocessing {
        cycle_slip: Some(CycleSlipPolicy::SplitArc),
        hatch_window_cap: None,
        elevation_mask_deg: None,
    };
    let arc = RtkArcConfig::new(
        reference,
        BaselineReferenceSelection::Auto,
        model,
        30.0,
        30.0,
        [0.0; 3],
        BTreeMap::new(),
        BTreeMap::new(),
        update,
        preprocessing,
    );
    let opts = ValidatedFixedSolveOpts {
        float: FloatSolveOpts {
            position_tol_m: 1.0e-4,
            ambiguity_tol_m: 1.0e-4,
            max_iterations: 10,
        },
        fixed: FixedSolveOpts {
            position_tol_m: 1.0e-4,
            ambiguity_tol_m: 1.0e-4,
            max_iterations: 10,
            ratio_threshold: 3.0,
            partial_ambiguity_resolution: true,
            partial_min_ambiguities: 4,
        },
        residual: ResidualValidationOpts {
            threshold_sigma: None,
            max_exclusions: 0,
        },
    };
    let carrier =
        StaticReferenceCarrierRinexOptions::new(arc_options, RtkStaticArcConfig::new(arc, opts));
    let options = StaticReferenceStationRinexOptions::new(None, Some(carrier), true);
    let solution =
        solve_static_reference_station_rinex(&sp3, &base_obs, &rover_obs, reference, &options)
            .expect("static reference solve");
    json!({
        "referencePositionM": hexes(&reference),
        "positionM": hexes(&solution.position.as_array()),
        "baselineVectorM": hexes(&solution.baseline_vector_m),
        "positionCovarianceEcefM2": solution
            .covariance
            .position_ecef_m2
            .iter()
            .map(|row| hexes(row))
            .collect::<Vec<_>>(),
        "heightM": hex(solution.geodetic.expect("geodetic").height_m),
        "usedMeasurements": solution.mode_reports[0].used_measurements,
    })
}

/// The TLE fit of the `tle_fit` test: the ISS element set (read under the
/// lenient checksum policy) propagated at seven epochs three minutes apart
/// about its epoch, then fitted with B*, velocity rows, a soft-L1 loss and
/// Jacobian scaling. The samples are part of the section, so the test fits the
/// same inputs.
fn tle_fit_section() -> Value {
    use sidereon::passes::{propagate_teme_arc, UtcInstant};
    use sidereon::sgp4::{
        fit_tle, FitConfig, FitSample, JulianDate, Loss, OpsMode, Satellite, TleMetadata, XScale,
    };
    use sidereon::tle::TlePolicy;

    const L1: &str = "1 25544U 98067A   18183.80969102  .00002605  00000-0  48194-4 0  9999";
    const L2: &str = "2 25544  51.6418 282.1100 0003956 227.7591 296.3436 15.54198036120477";
    let (truth, _) = Satellite::from_tle_with_policy(L1, L2, OpsMode::Improved, TlePolicy::Lenient)
        .expect("truth TLE");
    // The test forms the base epoch as
    // Date.UTC(2018, 0, 1) * 1000 + Math.round(183.80969102 * 86400e6).
    let base_us: i64 =
        1_514_764_800_000_000 + (183.809_691_02_f64 * 86_400_000_000.0).round() as i64;
    let epochs: Vec<i64> = [-180_i64, -120, -60, 0, 60, 120, 180]
        .iter()
        .map(|dt| base_us + dt * 1_000_000)
        .collect();
    let instants: Vec<UtcInstant> = epochs
        .iter()
        .map(|&us| UtcInstant::from_unix_microseconds(us))
        .collect();
    let arc = propagate_teme_arc(&truth, &instants).expect("truth arc");
    let samples: Vec<FitSample> = epochs
        .iter()
        .zip(&arc)
        .map(|(&us, state)| {
            // As the test splits it: 2440587.5 + us / 86400e6, whole part
            // truncated.
            let jd = 2_440_587.5 + us as f64 / 86_400_000_000.0;
            let whole = jd.trunc();
            FitSample {
                epoch: JulianDate(whole, jd - whole),
                position_teme_km: state.position,
                velocity_teme_km_s: Some(state.velocity),
            }
        })
        .collect();
    let mut config = FitConfig::default();
    config.fit_bstar = true;
    config.use_velocity = true;
    config.velocity_weight_s = Some(60.0);
    config.loss = Loss::SoftL1;
    config.f_scale = 1.0;
    config.x_scale = Some(XScale::Jac);
    config.max_nfev = Some(80);
    config.metadata = TleMetadata {
        catalog_number: 25544,
        classification: "U".to_string(),
        international_designator: "98067A".to_string(),
        element_set_number: 999,
        rev_at_epoch: 12047,
        object_name: "ISS (ZARYA)".to_string(),
    };
    let fit = fit_tle(&samples, &config).expect("TLE fit");
    let (fitted, _) = Satellite::from_tle_with_policy(
        &fit.line1,
        &fit.line2,
        OpsMode::Improved,
        TlePolicy::Strict,
    )
    .expect("fitted TLE");
    let check = propagate_teme_arc(&fitted, &instants[3..4]).expect("fitted arc");
    let e = &fit.omm.epoch;
    json!({
        "ommEpoch": [e.year, e.month, e.day, e.hour, e.minute, e.second, e.microsecond, e.femtosecond],
        "check": {
            "positionKm": hexes(&check[0].position),
            "velocityKmS": hexes(&check[0].velocity),
        },
        "samples": samples.iter().map(|sample| json!({
            "epoch": [hex(sample.epoch.0), hex(sample.epoch.1)],
            "positionTemeKm": hexes(&sample.position_teme_km),
            "velocityTemeKmS": hexes(&sample.velocity_teme_km_s.expect("velocity")),
        })).collect::<Vec<_>>(),
        "line1": fit.line1,
        "line2": fit.line2,
        "elements": serde_json::to_value(&fit.elements).expect("elements"),
        "stats": serde_json::to_value(&fit.stats).expect("stats"),
    })
}

/// The RINEX clock interpolation across the 2016-12-31 leap second of the
/// `rinex_clock` test: a UTC product with samples at 23:59:59, 23:59:60 and
/// 00:00:00, queried at 23:59:60.5.
fn clock_leap_section() -> Value {
    use sidereon_core::rinex::clock::{ClockEpoch, RinexClock};

    let header = |payload: &str, label: &str| format!("{payload:<60}{label}");
    let text = [
        header(
            "     3.00           CLOCK DATA          GPS",
            "RINEX VERSION / TYPE",
        ),
        header("   UTC", "TIME SYSTEM ID"),
        header("", "END OF HEADER"),
        "AS G05  2016 12 31 23 59 59.000000  1    0.100000000000E-03".to_string(),
        "AS G05  2016 12 31 23 59 60.000000  1    0.200000000000E-03".to_string(),
        "AS G05  2017 01 01 00 00  0.000000  1    0.300000000000E-03".to_string(),
        String::new(),
    ]
    .join("\n");
    let clock = RinexClock::parse(&text).expect("UTC clock product");
    let epoch = ClockEpoch {
        year: 2016,
        month: 12,
        day: 31,
        hour: 23,
        minute: 59,
        second: 60.5,
    };
    let bias = clock
        .clock_s("G05", epoch)
        .expect("clock query")
        .expect("bias inside the samples");
    json!({ "biasAt60p5": hex(bias) })
}

/// The header and per-system sections of the observation QC report of the
/// WTZR RINEX 3 fixture under the default QC options, built from the core
/// report's own fields. Floating-point values are JSON numbers, which
/// serde_json writes as their shortest round-tripping decimals.
fn qc_report_section() -> Value {
    use sidereon_core::observation_qc::{observation_qc_with_options, ObservationQcOptions};
    use sidereon_core::rinex::observations::RinexObs;

    let obs = RinexObs::parse(&fixture_text(
        "obs/WTZR00DEU_R_20201770000_01D_30S_MO_120epoch.rnx",
    ))
    .expect("WTZR OBS");
    let report =
        observation_qc_with_options(&obs, ObservationQcOptions::default()).expect("observation QC");
    let h = &report.header;
    let time = |t: &sidereon_core::observation_qc::ObservationQcTime| {
        json!({
            "epoch": {
                "year": t.epoch.year,
                "month": t.epoch.month,
                "day": t.epoch.day,
                "hour": t.epoch.hour,
                "minute": t.epoch.minute,
                "second": t.epoch.second,
            },
            "timeScale": t.time_scale,
        })
    };
    json!({
        "header": {
            "markerName": h.marker_name,
            "markerNumber": h.marker_number,
            "markerType": h.marker_type,
            "receiver": h.receiver.as_ref().map(|r| json!({
                "number": r.number,
                "receiverType": r.receiver_type,
                "version": r.version,
            })),
            "antenna": h.antenna.as_ref().map(|a| json!({
                "number": a.number,
                "antennaType": a.antenna_type,
            })),
            "approxPositionM": h.approx_position_m,
            "antennaDeltaHenM": h.antenna_delta_hen_m,
            "timeOfFirstObs": h.time_of_first_obs.as_ref().map(time),
            "timeOfLastObs": h.time_of_last_obs.as_ref().map(time),
            "durationS": h.duration_s,
        },
        "systems": report.systems.iter().map(|row| json!({
            "system": row.system.as_str(),
            "satellitesSeen": row.satellites_seen,
            "epochsWithObservations": row.epochs_with_observations,
            "valueObservations": row.value_observations,
            "expectedObservations": row.expected_observations,
            "completenessRatio": row.completeness_ratio,
            "gapCount": row.gap_count,
            "totalGapS": row.total_gap_s,
        })).collect::<Vec<_>>(),
    })
}

fn optional_hexes(values: &[Option<f64>]) -> Vec<Option<String>> {
    values.iter().map(|value| value.map(hex)).collect()
}

fn emission_batch_json(batch: &EmissionMediaBatch) -> Value {
    json!({
        "positionsEcefM": batch
            .positions_ecef_m
            .iter()
            .map(|position| position.as_ref().map(|position| hexes(position)))
            .collect::<Vec<_>>(),
        "clocksS": optional_hexes(&batch.clocks_s),
        "ionosphereSlantDelaysM": optional_hexes(&batch.ionosphere_slant_delays_m),
        "troposphereDelaysM": optional_hexes(&batch.troposphere_delays_m),
    })
}

/// `test/emission_media.test.mjs`: the GRG product at its 41st epoch from a
/// receiver at 48 N, 11 E, 600 m, with the default troposphere correction; the
/// second batch adds a 1.5 rad elevation cutoff. The receiver and epoch are
/// recorded so the test hands the binding these exact values.
fn emission_media_section() -> Value {
    let sp3 = load_sp3(GRG_SP3);
    let epoch = sp3.epochs_j2000_seconds()[40];
    let receiver = geodetic_to_ecef(48.0, 11.0, 600.0);
    let satellites: Vec<GnssSatelliteId> = ["G16", "E01", "C01"]
        .iter()
        .map(|token| token.parse().expect("satellite token"))
        .collect();
    let mut options = EmissionMediaBatchOptions::default();
    options.media.troposphere = Some(ObservableTroposphereCorrection::default());
    let all = emission_media_batch_at_j2000_s(&sp3, &satellites, &[epoch; 3], receiver, options)
        .expect("emission media batch");
    let mut cutoff = options;
    cutoff.min_elevation_rad = Some(1.5);
    let below = emission_media_batch_at_j2000_s(&sp3, &satellites[..1], &[epoch], receiver, cutoff)
        .expect("emission media batch below cutoff");
    json!({
        "epochJ2000S": hex(epoch),
        "receiverEcefM": hexes(&receiver),
        "all": emission_batch_json(&all),
        "belowCutoff": emission_batch_json(&below),
    })
}

/// `test/precise_interpolant_artifact.test.mjs`: the GRG product's
/// precise-interpolant artifact, its length and checksum, and G16 evaluated on
/// the 11th epoch and midway to the 12th.
fn precise_artifact_section() -> Value {
    let sp3 = load_sp3(GRG_SP3);
    let bytes = sp3
        .precise_interpolant_store_bytes()
        .expect("precise interpolant artifact bytes");
    let byte_length = bytes.len();
    let artifact = MmapPreciseEphemerisInterpolant::from_vec(bytes).expect("open precise artifact");
    let epochs = sp3.epochs_j2000_seconds();
    let queries = [epochs[10], 0.5 * (epochs[10] + epochs[11])];
    let g16: GnssSatelliteId = "G16".parse().expect("satellite token");
    let states = queries
        .iter()
        .map(|&query| {
            let state = artifact
                .position_at_j2000_seconds(g16, query)
                .expect("artifact state");
            json!({
                "queryJ2000S": hex(query),
                "positionM": hexes(&state.position.as_array()),
                "clockS": state.clock_s.map(hex),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "byteLength": byte_length,
        "checksum64": artifact.checksum64().to_string(),
        "g16": states,
    })
}

/// The sidereon-core commit `SIDEREON_CORE_REV` names. Exits with a message
/// when it is unset or not 7 to 40 hexadecimal digits.
fn core_rev() -> String {
    let rev = match std::env::var("SIDEREON_CORE_REV") {
        Ok(rev) => rev,
        Err(_) => {
            eprintln!(
                "golden-gen: set SIDEREON_CORE_REV to the sidereon-core commit this build is \
                 patched to (see the crate documentation)"
            );
            std::process::exit(2);
        }
    };
    let valid = (7..=40).contains(&rev.len()) && rev.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        eprintln!("golden-gen: SIDEREON_CORE_REV={rev:?} is not a 7 to 40 digit commit id");
        std::process::exit(2);
    }
    rev
}

fn main() {
    let rev = core_rev();
    let (velocity, range_rates) = velocity_section();
    let doc = json!({
        "source": format!(
            "test/golden-gen: the test scenarios reproduced natively against sidereon-core {rev}"
        ),
        "coreRev": rev,
        "velocity": velocity,
        "sppDoppler": spp_doppler_section(&range_rates),
        "static": static_section(),
        "sppTrace": spp_trace_section(),
        "raimSolution": raim_solution_section(),
        "navStore": nav_store_section(),
        "navRepair": nav_repair_section(),
        "bias": bias_section(),
        "scenario": scenario_section(),
        "fusionTight": fusion_tight_section(),
        "ppp": ppp_section(),
        "rtkStaticReference": rtk_static_reference_section(),
        "tleFit": tle_fit_section(),
        "clockLeapInterp": clock_leap_section(),
        "qcReport": qc_report_section(),
        "emissionMedia": emission_media_section(),
        "preciseArtifact": precise_artifact_section(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&doc).expect("serialize goldens")
    );
}
