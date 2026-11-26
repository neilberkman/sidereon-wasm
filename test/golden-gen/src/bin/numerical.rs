//! Reference states, covariance and decay for numerical propagation scenarios.

use std::sync::Arc;

use serde_json::{json, Value};
use sidereon_core::astro::covariance::Covariance6;
use sidereon_core::astro::forces::{
    DragParameters, EmpiricalHarmonic, EmpiricalRtnAcceleration, ForceModel, RtnAxis, SpaceWeather,
    SpaceWeatherSource, SphericalHarmonicGravityConfig,
};
use sidereon_core::astro::frames::{
    Iers2010EarthOrientationProvider, Iers2010EopSample, TdbEarthOrientationProvider,
};
use sidereon_core::astro::propagator::{
    estimate_decay_with_context, estimate_decay_with_source,
    estimate_decay_with_source_and_context, CovarianceFrame, CovariancePropagationOptions,
    DecayConfig, DecayEstimate, ForceModelComponents, ForceModelKind, IntegratorKind,
    IntegratorOptions, LabeledCovariance6, ProcessNoise, PropagationContext, StatePropagator,
};
use sidereon_core::astro::space_weather;
use sidereon_core::astro::state::CartesianState;
use sidereon_core::astro::time::{
    civil::j2000_seconds, tdb_from_tt_j2000_seconds, tt_from_tdb_j2000_seconds,
};

fn hex(value: f64) -> String {
    format!("0x{:016x}", value.to_bits())
}

fn state(state: &CartesianState) -> Value {
    json!({
        "positionKm": state.position_array().map(hex),
        "velocityKmS": state.velocity_array().map(hex),
    })
}

fn arc() -> Value {
    let position: [f64; 3] = [7000.0, 0.0, 0.0];
    let velocity: [f64; 3] = [0.5, 8.2, 1.1];
    let mut options = IntegratorOptions::default();
    options.abs_tol = 1e-12;
    options.rel_tol = 1e-12;
    options.initial_step = 60.0;
    options.min_step = 1e-6;
    options.max_step = 3600.0;
    options.max_steps = 1_000_000;
    options.dense_output = false;
    let propagator = StatePropagator::new(
        0.0,
        position,
        velocity,
        ForceModelKind::two_body_j2(),
        IntegratorKind::Dp54,
    )
    .with_options(options);
    let times: Vec<_> = (0..10).map(|i| f64::from(i) * 600.0).collect();
    let states = propagator
        .ephemeris(&times)
        .expect("sample the reference arc");
    let samples: Vec<_> = times
        .iter()
        .zip(states)
        .map(|(&time, sample)| {
            json!({
                "time_s": time,
                "time_s_hex": hex(time),
                "position_km_hex": sample.position_array().map(hex),
                "velocity_km_s_hex": sample.velocity_array().map(hex),
            })
        })
        .collect();
    json!({
        "source": "reference_arc_is_deterministic_and_frozen",
        "force_model": "two_body_j2",
        "integrator": "dp54",
        "options": {
            "abs_tol": options.abs_tol,
            "rel_tol": options.rel_tol,
            "initial_step_s": options.initial_step,
            "min_step_s": options.min_step,
            "max_step_s": options.max_step,
            "max_steps": options.max_steps,
        },
        "epoch_s_hex": hex(0.0),
        "position_km_hex": position.map(hex),
        "velocity_km_s_hex": velocity.map(hex),
        "samples": samples,
    })
}

fn geopotential() -> Value {
    let force = ForceModelComponents::earth_two_body().with_spherical_harmonic(
        SphericalHarmonicGravityConfig::earth(2, 0).expect("EGM96 degree 2"),
    );
    let mut options = IntegratorOptions::default();
    options.initial_step = 30.0;
    options.max_step = 30.0;
    let context = PropagationContext::new()
        .with_body_fixed_frame_provider(Arc::new(TdbEarthOrientationProvider::new()));
    let states = StatePropagator::new(
        0.0,
        [7000.0, -1210.0, 1300.0],
        [1.0, 7.2, 0.5],
        ForceModelKind::composite(force),
        IntegratorKind::Rk4,
    )
    .with_options(options)
    .ephemeris_with_context(&[0.0, 60.0, 120.0], &context)
    .expect("sample geopotential arc");
    state(&states[2])
}

fn covariance() -> Value {
    let mut options = IntegratorOptions::default();
    options.initial_step = 2.0;
    options.max_step = 2.0;
    let propagator = StatePropagator::new(
        0.0,
        [7000.0, 0.0, 0.0],
        [0.0, 7.546049108166282, 0.0],
        ForceModelKind::two_body(),
        IntegratorKind::Rk4,
    )
    .with_options(options);
    let covariance = Covariance6::from_diagonal([1e-4, 2e-4, 3e-4, 1e-8, 2e-8, 3e-8])
        .expect("diagonal covariance");
    let mut covariance_options = CovariancePropagationOptions::default();
    covariance_options.process_noise = ProcessNoise::RtnAccelerationPsd {
        q_radial_km2_s3: 1e-12,
        q_transverse_km2_s3: 2e-12,
        q_normal_km2_s3: 3e-12,
    };
    let ephemeris = propagator
        .propagate_covariance(
            LabeledCovariance6 {
                covariance,
                frame: CovarianceFrame::Inertial,
            },
            &[0.0, 60.0, 120.0],
            &covariance_options,
        )
        .expect("sample covariance arc");
    let nodes = ephemeris.nodes();
    let mid = ephemeris
        .covariance_at(60.0)
        .expect("covariance at second epoch");
    json!({
        "finalState": state(&nodes[2].state),
        "finalDiagonal": std::array::from_fn::<_, 6, _>(|i| hex(nodes[2].covariance.as_matrix()[i][i])),
        "midDiagonal": std::array::from_fn::<_, 6, _>(|i| hex(mid.as_matrix()[i][i])),
    })
}

fn space_weather_source() -> SpaceWeatherSource {
    let table = space_weather::parse(include_bytes!(
        "../../../fixtures/space_weather/SW-All-20260702-trim.csv"
    ))
    .expect("space weather records")
    .value;
    SpaceWeatherSource::Table(Arc::new(table))
}

fn weather() -> SpaceWeather {
    SpaceWeather {
        f107: 150.0,
        f107a: 150.0,
        ap: 12.0,
    }
}

fn context() -> PropagationContext {
    let samples = [52941, 52942, 52943]
        .map(|mjd_utc| Iers2010EopSample {
            mjd_utc,
            xp_arcsec: 0.1,
            yp_arcsec: 0.2,
            ut1_minus_utc_s: -0.2,
            lod_s: 0.001,
            dx_arcsec: 0.0,
            dy_arcsec: 0.0,
            pole_rates_rad_s: None,
        })
        .to_vec();
    PropagationContext::new().with_body_fixed_frame_provider(Arc::new(
        Iers2010EarthOrientationProvider::new(samples).expect("IERS orientation"),
    ))
}

fn context_propagation() -> Value {
    let context = context();
    let epoch = j2000_seconds(2003, 10, 30, 12, 0, 0.0);
    let epochs = [epoch, epoch + 60.0, epoch + 120.0];
    let mut options = IntegratorOptions::default();
    options.initial_step = 2.0;
    options.max_step = 2.0;
    let mut propagator = StatePropagator::new(
        epoch,
        [6770.0, 170.0, 200.0],
        [-0.2, 7.55, 0.8],
        ForceModelKind::earth_phase_b(8, 8, None).expect("phase B force"),
        IntegratorKind::Rk4,
    )
    .with_options(options);
    let end = epochs[2];
    let path = |propagator: &StatePropagator, forces: &[Arc<dyn ForceModel>], ctx| {
        propagator
            .ephemeris_with_additional_forces(&epochs, forces, ctx)
            .expect("sample the trajectory")
            .iter()
            .map(state)
            .collect::<Vec<_>>()
    };
    let baseline_context = PropagationContext::new()
        .with_body_fixed_frame_provider(Arc::new(TdbEarthOrientationProvider::new()));
    let positions = path(&propagator, &[], &context);
    let baseline = path(&propagator, &[], &baseline_context);
    let stm = propagator
        .state_transition_matrix_to_with_context(end, &context)
        .expect("state transition");
    let covariance = Covariance6::from_diagonal([1e-4, 2e-4, 3e-4, 1e-8, 2e-8, 3e-8])
        .expect("diagonal covariance");
    let mut covariance_options = CovariancePropagationOptions::default();
    covariance_options.process_noise = ProcessNoise::RtnAccelerationPsd {
        q_radial_km2_s3: 1e-12,
        q_transverse_km2_s3: 2e-12,
        q_normal_km2_s3: 3e-12,
    };
    let covariance_path = propagator
        .propagate_covariance_with_context(
            LabeledCovariance6 {
                covariance,
                frame: CovarianceFrame::Inertial,
            },
            &epochs,
            &covariance_options,
            &context,
        )
        .expect("sample the covariance");
    let (pair_state, pair_covariance) = propagator
        .propagate_state_with_covariance_with_context(covariance, 120.0, &context)
        .expect("propagate state and covariance");
    let additional: Vec<Arc<dyn ForceModel>> = vec![
        Arc::new(
            EmpiricalRtnAcceleration::constant_terms()
                .with_term(RtnAxis::Radial, EmpiricalHarmonic::Constant, 1e-7)
                .unwrap(),
        ),
        Arc::new(
            EmpiricalRtnAcceleration::constant_terms()
                .with_term(RtnAxis::CrossTrack, EmpiricalHarmonic::Constant, 3e-7)
                .unwrap(),
        ),
    ];
    propagator.drag = Some(
        DragParameters::from_bc_factor_m2_kg(0.02, weather(), 100.0).expect("drag parameters"),
    );
    let fixed = path(&propagator, &additional, &context);
    propagator.space_weather = Some(space_weather_source());
    let table = path(&propagator, &additional, &context);
    let final_state = propagator
        .propagate_to_with_additional_forces(end, &additional, &context)
        .expect("propagate with drag and additional forces")
        .final_state;
    json!({
        "epoch": epoch,
        "baseline": baseline,
        "states": positions,
        "stm": stm.into_iter().flatten().map(hex).collect::<Vec<_>>(),
        "covariance": covariance_path.nodes().iter().flat_map(|node| {
            node.covariance.as_matrix().iter().flatten().copied().map(hex)
        }).collect::<Vec<_>>(),
        "pairState": state(&pair_state),
        "pairCovariance": pair_covariance.as_matrix().iter().flatten().copied().map(hex).collect::<Vec<_>>(),
        "fixed": fixed,
        "table": table,
        "finalState": state(&final_state),
    })
}

fn decay_reference(result: DecayEstimate) -> Value {
    json!({
        "timeToDecayS": hex(result.time_to_decay_s),
        "reentryEpochS": hex(result.reentry_state.epoch_tdb_seconds),
        "reentryState": state(&result.reentry_state),
        "reentryAltitudeKm": hex(result.reentry_altitude_km),
    })
}

fn decay() -> Value {
    let drag = DragParameters::from_bc_factor_m2_kg(0.02, SpaceWeather::default(), 100.0)
        .expect("drag parameters");
    let config = DecayConfig::new(drag)
        .with_scan_step_s(30.0)
        .with_max_duration_s(7200.0)
        .with_max_scan_samples(300)
        .with_reentry_altitude_km(100.0)
        .with_crossing_tolerance_s(1.0);
    let initial = CartesianState::new(
        j2000_seconds(2003, 10, 30, 0, 0, 0.0),
        [6478.3, 0.0, 0.0],
        [0.0, 7.85, 0.0],
    );
    decay_reference(
        estimate_decay_with_source(initial, &config, &space_weather_source())
            .expect("reentry estimate"),
    )
}

fn context_decay() -> Value {
    let drag =
        DragParameters::from_bc_factor_m2_kg(0.02, weather(), 100.0).expect("drag parameters");
    let config = DecayConfig::new(drag)
        .with_scan_step_s(30.0)
        .with_max_duration_s(7200.0)
        .with_max_scan_samples(300)
        .with_reentry_altitude_km(100.0)
        .with_crossing_tolerance_s(1.0);
    let initial = CartesianState::new(
        j2000_seconds(2003, 10, 30, 12, 0, 0.0),
        [6478.3, 0.0, 0.13],
        [0.0, 7.85, 0.01],
    );
    let context = context();
    let source = space_weather_source();
    json!({
        "fixed": decay_reference(estimate_decay_with_context(initial, &config, &context)
            .expect("reentry with IERS orientation")),
        "table": decay_reference(estimate_decay_with_source_and_context(initial, &config, &source, &context)
            .expect("reentry with weather and IERS orientation")),
        "baselineFixed": decay_reference(estimate_decay_with_context(initial, &config, &PropagationContext::new())
            .expect("reentry with default orientation")),
        "baselineTable": decay_reference(estimate_decay_with_source(initial, &config, &source)
            .expect("reentry with weather and default orientation")),
    })
}

fn main() {
    let document = match std::env::args().nth(1).as_deref() {
        Some("arc") => arc(),
        None => {
            let period = 2.0 * std::f64::consts::PI * (7000.0_f64.powi(3) / 398600.4418).sqrt();
            json!({
                "geopotential": geopotential(),
                "covariance": covariance(),
                "decay": decay(),
                "contextPropagation": context_propagation(), "contextDecay": context_decay(),
                "onePeriodTdbSeconds": hex(tdb_from_tt_j2000_seconds(tt_from_tdb_j2000_seconds(0.0) + period)),
            })
        }
        _ => panic!("expected arc or no argument"),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&document).expect("serialize reference data")
    );
}
