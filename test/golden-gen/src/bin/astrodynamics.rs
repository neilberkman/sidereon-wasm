//! Reference values for ephemeris, eclipse and orbit model scenarios.

use serde_json::{json, Value};
use sidereon::passes::UtcInstant;
use sidereon_core::astro::attitude::gnss;
use sidereon_core::astro::bodies::sun_moon;
use sidereon_core::astro::de::{DeBody, DeEphemeris};
use sidereon_core::astro::events::eclipse;
use sidereon_core::astro::forces::{self, ForceModel};
use sidereon_core::astro::frames::{
    self, EarthOrientationProvider, Iers2010EarthOrientationProvider, Iers2010EopSample,
};
use sidereon_core::astro::propagator::{
    analytic, propagate_variational, IntegratorKind, IntegratorOptions, PropagationContext,
    VariationalEpoch, VariationalEquations,
};
use sidereon_core::astro::state::CartesianState;
use sidereon_core::astro::time::{
    tdb_from_tt_j2000_seconds, tdb_minus_tt_seconds, tt_from_tdb_j2000_seconds,
};

fn solar() -> Value {
    let scales = UtcInstant::from_unix_microseconds(1_593_043_200_000_000)
        .try_time_scales()
        .expect("time scales");
    let at = sun_moon::sun_moon_gcrs_at(&scales).expect("GCRS at UTC");
    let by_century = sun_moon::sun_moon_gcrs(0.25).expect("GCRS at TT centuries");
    let by_seconds = sun_moon::sun_moon_gcrs_at_tdb_seconds(1000.0).expect("GCRS at TDB");
    json!({
        "at": [at.sun, at.moon],
        "byCentury": [by_century.sun, by_century.moon],
        "bySeconds": [by_seconds.sun, by_seconds.moon],
        "toMeanOfDate": frames::transforms::gcrs_to_mean_of_date_matrix(&scales).unwrap().concat(),
        "toGcrs": frames::transforms::mean_of_date_to_gcrs_matrix(&scales).unwrap().concat(),
        "tdb": tdb_from_tt_j2000_seconds(1000.0),
        "tt": tt_from_tdb_j2000_seconds(1000.0),
        "tdbMinusTt": tdb_minus_tt_seconds(1000.0),
    })
}

fn de() -> Value {
    let ephemeris = DeEphemeris::from_bytes(include_bytes!("../../../fixtures/bodies/de440.440"))
        .expect("DE440 header and first data record");
    let coverage = ephemeris.coverage_tdb_seconds();
    let states = ephemeris
        .sun_moon_at_tdb_seconds(coverage.0)
        .expect("state at record start");
    json!({
        "coverage": [coverage.0, coverage.1],
        "constantCount": ephemeris.constants().len(),
        "au": ephemeris.astronomical_unit_m(),
        "earthMoonMassRatio": ephemeris.earth_moon_mass_ratio(),
        "sunGm": ephemeris.gm_m3_s2(DeBody::Sun),
        "sun": [states.sun.position_m, states.sun.velocity_m_s],
        "moon": [states.moon.position_m, states.moon.velocity_m_s],
    })
}

fn eop() -> Value {
    let samples = [59025, 59026, 59027].map(|mjd_utc| Iers2010EopSample {
        mjd_utc,
        xp_arcsec: 0.1,
        yp_arcsec: 0.2,
        ut1_minus_utc_s: -0.25,
        lod_s: 0.001,
        dx_arcsec: 0.0001,
        dy_arcsec: -0.0002,
        pole_rates_rad_s: Some([1e-15, -2e-15]),
    });
    let provider = Iers2010EarthOrientationProvider::new(samples.to_vec()).expect("daily EOP");
    let orientation = provider
        .orientation_at_utc_mjd(59026, 0.0)
        .expect("orientation at midnight");
    let epoch_tdb = tdb_from_tt_j2000_seconds((59026.0 - 51544.5) * 86400.0 + 70.0);
    let by_tdb = provider.orientation_at_tdb_seconds(epoch_tdb).unwrap();
    json!({
        "quaternion": orientation.iers2010_quaternion(),
        "rotation": orientation.earth_rotation_vector_itrf_rad_s(),
        "gcrfToItrf": orientation.gcrf_to_itrf_matrix().concat(),
        "itrfToGcrf": orientation.itrf_to_gcrf_matrix().concat(),
        "gcrfToItrfRate": orientation.gcrf_to_itrf_rotation_rate_matrix().concat(),
        "itrfToGcrfRate": orientation.itrf_to_gcrf_rotation_rate_matrix().concat(),
        "epochTdb": epoch_tdb,
        "gcrfToItrfAtTdb": by_tdb.gcrf_to_itrf_matrix().concat(),
    })
}

fn attitude() -> Value {
    let position = [26_500_000.0, 0.0, 0.0];
    let velocity = [0.0, 3000.0, 2000.0];
    let sun = [1.0e11, 2.0e10, 1.0e10];
    let yaw = gnss::nominal_yaw(position, velocity, sun).expect("nominal yaw");
    let dyb = gnss::dyb_axes(position, sun).expect("solar axes");
    json!({
        "yaw": yaw.yaw_rad, "beta": yaw.beta_rad,
        "body": [yaw.body_x, yaw.body_y, yaw.body_z],
        "dyb": [dyb.d, dyb.y, dyb.b]
    })
}

fn analytic_orbits() -> Value {
    let gravity = analytic::ZonalHarmonics::earth();
    let circular = analytic::CircularElements {
        a_km: 7000.0,
        ex: 0.001,
        ey: 0.002,
        i_rad: 0.9,
        raan_rad: 0.2,
        mean_latitude_argument_rad: 0.3,
    };
    let keplerian = analytic::KeplerianElements {
        a_km: 8000.0,
        e: 0.1,
        i_rad: 0.9,
        arg_perigee_rad: 0.2,
        raan_rad: 0.3,
        mean_anomaly_rad: 0.4,
    };
    let eh =
        analytic::EcksteinHechlerPropagator::from_mean_elements(0.0, circular, &gravity).unwrap();
    let bl = analytic::BrouwerLyddanePropagator::from_mean_elements(0.0, keplerian, &gravity, 0.0)
        .unwrap();
    let eh_state = eh.propagate(60.0).unwrap();
    let bl_state = bl.propagate(60.0).unwrap();
    let (eh_r, eh_v) = circular.to_cartesian(gravity.mu_km3_s2).unwrap();
    let (bl_r, bl_v) = keplerian.to_cartesian(gravity.mu_km3_s2).unwrap();
    let eh_mean = analytic::eckstein_hechler_mean_elements(
        &eh_state,
        &gravity,
        analytic::MeanConversionOptions::ECKSTEIN_HECHLER_DEFAULT,
    )
    .unwrap();
    let bl_mean = analytic::brouwer_lyddane_mean_elements(
        &bl_state,
        &gravity,
        analytic::MeanConversionOptions::BROUWER_LYDDANE_DEFAULT,
    )
    .unwrap();
    let circular_values = |e: analytic::CircularElements| {
        [
            e.a_km,
            e.ex,
            e.ey,
            e.i_rad,
            e.raan_rad,
            e.mean_latitude_argument_rad,
        ]
    };
    let keplerian_values = |e: analytic::KeplerianElements| {
        [
            e.a_km,
            e.e,
            e.i_rad,
            e.arg_perigee_rad,
            e.raan_rad,
            e.mean_anomaly_rad,
        ]
    };
    json!({
        "gravity": [gravity.mu_km3_s2, gravity.re_km, gravity.c20, gravity.c30, gravity.c40, gravity.c50, gravity.c60],
        "circularCartesian": [eh_r, eh_v],
        "eccentricity": circular.eccentricity(),
        "ehOsculating": circular_values(eh.osculating_elements(0.0).unwrap()),
        "blOsculating": keplerian_values(bl.osculating_elements(0.0).unwrap()),
        "ehMean": {"iterations":eh_mean.iterations,"elements":circular_values(eh_mean.elements)},
        "blMean": {"iterations":bl_mean.iterations,"elements":keplerian_values(bl_mean.elements)},
        "keplerianCartesian": [bl_r, bl_v],
        "eh": [eh_state.position_array(), eh_state.velocity_array()],
        "bl": [bl_state.position_array(), bl_state.velocity_array()],
    })
}

fn ecom_partials(
    state: forces::AccelerationPartials,
    parameters: impl IntoIterator<Item = impl Into<[f64; 3]>>,
) -> Value {
    let position: Vec<f64> = state.wrt_position.transpose().as_slice().to_vec();
    let velocity: Vec<f64> = state.wrt_velocity.transpose().as_slice().to_vec();
    let parameters: Vec<f64> = parameters.into_iter().flat_map(Into::into).collect();
    json!([position, velocity, parameters])
}

fn radiation() -> Value {
    let r = [26_500.0, 100.0, 200.0];
    let v = [0.0, 3.0, 2.0];
    let sun = [1.0e8, 2.0e7, 1.0e7];
    let moon = [-380_000.0, 0.0, 0.0];
    let ecom1 =
        forces::Ecom1SolarRadiationPressure::new(forces::Ecom1Coefficients::from_array([1e-7; 9]))
            .unwrap();
    let ecom2 = forces::Ecom2SolarRadiationPressure::d2b1(
        forces::Ecom2D2b1Coefficients::from_array([1e-7; 7]),
    )
    .unwrap();
    let ecom1_sun = ecom1.clone().with_moon_eclipse(false);
    let ecom2_sun = ecom2.clone().with_moon_eclipse(false);
    let srp = forces::SolarRadiationPressure::new(1.2, 0.02).unwrap();
    let earth = forces::EarthRadiationPressure::new(1.2, 0.02).unwrap();
    let state = CartesianState::new(1000.0, r, v);
    let context = PropagationContext::new();
    let geometry = forces::ecom_geometry_with_shadow(
        r.into(),
        v.into(),
        sun.into(),
        Some(moon.into()),
        eclipse::EarthShadowModel::Wgs84Oblate,
    )
    .unwrap();
    let solar_geometry = forces::ecom_geometry(r.into(), v.into(), sun.into()).unwrap();
    json!({
        "ecom1": <[f64; 3]>::from(ecom1.acceleration_with_sun_moon(r.into(), v.into(), sun.into(), moon.into()).unwrap()),
        "ecom2": <[f64; 3]>::from(ecom2.acceleration_with_sun_moon(r.into(), v.into(), sun.into(), moon.into()).unwrap()),
        "ecom1Sun": <[f64; 3]>::from(ecom1_sun.acceleration_with_sun(r.into(), v.into(), sun.into()).unwrap()),
        "ecom2Sun": <[f64; 3]>::from(ecom2_sun.acceleration_with_sun(r.into(), v.into(), sun.into()).unwrap()),
        "ecom1PartialsSun": ecom_partials(
            ecom1_sun.acceleration_partials_with_sun(r.into(), v.into(), sun.into()).unwrap(),
            ecom1_sun.parameter_partials_with_sun(r.into(), v.into(), sun.into()).unwrap(),
        ),
        "ecom1PartialsSunMoon": ecom_partials(
            ecom1.acceleration_partials_with_sun_moon(r.into(), v.into(), sun.into(), moon.into()).unwrap(),
            ecom1.parameter_partials_with_sun_moon(r.into(), v.into(), sun.into(), moon.into()).unwrap(),
        ),
        "ecom2PartialsSun": ecom_partials(
            ecom2_sun.acceleration_partials_with_sun(r.into(), v.into(), sun.into()).unwrap(),
            ecom2_sun.parameter_partials_with_sun(r.into(), v.into(), sun.into()).unwrap(),
        ),
        "ecom2PartialsSunMoon": ecom_partials(
            ecom2.acceleration_partials_with_sun_moon(r.into(), v.into(), sun.into(), moon.into()).unwrap(),
            ecom2.parameter_partials_with_sun_moon(r.into(), v.into(), sun.into(), moon.into()).unwrap(),
        ),
        "geometrySun": { "dyb": [solar_geometry.dyb.d, solar_geometry.dyb.y, solar_geometry.dyb.b],
            "u": solar_geometry.argument_of_latitude_rad, "delta": solar_geometry.delta_u_rad, "light": solar_geometry.lighting_ratio },
        "srp": <[f64; 3]>::from(srp.acceleration(&state, &context).unwrap()),
        "earth": <[f64; 3]>::from(earth.acceleration_with_sun_moon(&state, sun.into(), moon.into()).unwrap()),
        "geometry": { "dyb": [geometry.dyb.d, geometry.dyb.y, geometry.dyb.b],
            "u": geometry.argument_of_latitude_rad, "delta": geometry.delta_u_rad, "light": geometry.lighting_ratio },
    })
}

fn variational() -> Value {
    let force = forces::EmpiricalRtnAcceleration::constant_terms()
        .with_term(
            forces::RtnAxis::Radial,
            forces::EmpiricalHarmonic::Constant,
            1e-7,
        )
        .unwrap();
    let initial = CartesianState::new(0.0, [7000.0, 0.0, 0.0], [0.0, 7.5, 0.0]);
    let epochs = [VariationalEpoch::from_tdb_seconds(0.0, 60.0)];
    let equations = VariationalEquations {
        force: &force,
        parameters: &[1e-7, 0.0, 0.0],
        sensitivity_parameters: &[0],
    };
    let states = propagate_variational(
        initial,
        &epochs,
        &equations,
        IntegratorKind::Dp54,
        &IntegratorOptions::default(),
        &PropagationContext::new(),
    )
    .unwrap();
    let state = &states[0];
    let stm = &state.state_transition;
    let matrix: Vec<f64> = (0..6)
        .flat_map(|row| (0..6).map(move |column| stm[(row, column)]))
        .collect();
    json!({
        "elapsedTt": epochs[0].elapsed_tt_s,
        "position": state.state.position_array(),
        "velocity": state.state.velocity_array(),
        "stm": matrix,
        "sensitivity": state.sensitivity.as_slice(),
    })
}

fn diagnostic_number(value: f64) -> Value {
    json!({"decimal":value.to_string(),"bitsHex":format!("{:016x}",value.to_bits())})
}

fn parameter_fit() -> Value {
    use sidereon_core::astro::propagator::{ForceModelComponents, ForceModelKind};
    use sidereon_core::astro::time::model::{Instant, TimeScale};
    use sidereon_core::orbit_determination::{
        fit_inertial_orbit_parameters, InertialPositionSample, OrbitParameterCovariance,
        OrbitParameterFitOptions,
    };
    use std::sync::Arc;

    use sidereon_core::astro::time::{civil::j2000_seconds_from_split, TimeScales};

    let times = [0, 1, 2, 3].map(|minute| {
        let scales = TimeScales::from_scale(TimeScale::Tt, 2000, 1, 1, 12, minute, 0.0).unwrap();
        (
            j2000_seconds_from_split(scales.jd_whole, scales.tdb_fraction),
            j2000_seconds_from_split(scales.jd_whole, scales.tt_fraction),
        )
    });
    let initial = CartesianState::new(times[0].0, [7000.0, 1.0, 2.0], [0.0, 7.5, 1.0]);
    let truth = forces::EmpiricalRtnAcceleration::new()
        .with_term(
            forces::RtnAxis::Radial,
            forces::EmpiricalHarmonic::Constant,
            1e-7,
        )
        .unwrap();
    let epochs = times.map(|(tdb_j2000_s, tt)| VariationalEpoch {
        tdb_j2000_s,
        elapsed_tt_s: tt - times[0].1,
    });
    let states = propagate_variational(
        initial,
        &epochs,
        &VariationalEquations {
            force: &truth,
            parameters: &[1e-7],
            sensitivity_parameters: &[],
        },
        IntegratorKind::Dp54,
        &IntegratorOptions::default(),
        &PropagationContext::new(),
    )
    .unwrap();
    let nanos = [0_i128, 60_000_000_000, 120_000_000_000, 180_000_000_000];
    let samples: Vec<_> = nanos
        .into_iter()
        .zip(states)
        .map(|(nanos, node)| InertialPositionSample {
            epoch: Instant::from_nanos(TimeScale::Tt, nanos),
            position_gcrf_m: node
                .state
                .position_array()
                .map(|coordinate| 1000.0 * coordinate),
        })
        .collect();

    let mut options = OrbitParameterFitOptions::default();
    options.force_model = ForceModelKind::composite(ForceModelComponents::EMPTY);
    options.additional_forces = vec![Arc::new(forces::EmpiricalRtnAcceleration::with_zero_terms(
        &[(forces::RtnAxis::Radial, forces::EmpiricalHarmonic::Constant)],
    ))];
    let name = "empirical.radial.constant";
    options.estimated_parameters = vec![name.to_owned()];
    let satellite = "G02".parse().unwrap();
    let fit = fit_inertial_orbit_parameters(&samples, satellite, Some(initial), &options).unwrap();
    use sidereon_core::orbit_determination::OrbitParameterFitError;

    let mut ill_conditioned = options.clone();
    ill_conditioned.max_condition_number = 2.0;
    let ill_conditioned = match fit_inertial_orbit_parameters(
        &samples,
        satellite,
        Some(initial),
        &ill_conditioned,
    )
    .unwrap_err()
    {
        OrbitParameterFitError::IllConditionedNormalEquations {
            condition_number,
            limit,
            weakest,
            ..
        } => {
            json!({"conditionNumber": diagnostic_number(condition_number), "limit": diagnostic_number(limit), "weakest": weakest})
        }
        error => panic!("expected ill-conditioned equations: {error}"),
    };
    let mut nonconvergent = options.clone();
    nonconvergent.max_iterations = 1;
    nonconvergent.convergence_threshold = 1e-20;
    let nonconvergent = match fit_inertial_orbit_parameters(
        &samples,
        satellite,
        Some(initial),
        &nonconvergent,
    )
    .unwrap_err()
    {
        OrbitParameterFitError::DidNotConverge {
            iterations,
            last_step_sigmas,
            ..
        } => {
            json!({"iterations": iterations, "lastStepSigmas": diagnostic_number(last_step_sigmas)})
        }
        error => panic!("expected nonconvergence: {error}"),
    };
    let variance = match fit.covariance {
        OrbitParameterCovariance::Estimated {
            variance_factor,
            degrees_of_freedom,
            ..
        } => [variance_factor, degrees_of_freedom as f64],
        OrbitParameterCovariance::Unbounded => panic!("expected finite covariance"),
    };
    json!({
        "illConditioned": ill_conditioned, "nonconvergent": nonconvergent,
        "positionsM": samples.iter().map(|sample| sample.position_gcrf_m).collect::<Vec<_>>(),
        "nanos": nanos.map(|value| value.to_string()),
        "seed": [initial.position_array(), initial.velocity_array()], "seedEpoch": initial.epoch_tdb_seconds,
        "state": [fit.initial_state.position_array(), fit.initial_state.velocity_array()],
        "stateEpoch": fit.initial_state.epoch_tdb_seconds,
        "iterations": fit.iterations, "conditionNumber": fit.condition_number, "variance": variance,
        "parameter": fit.force_parameter(name).unwrap().value, "sigma": fit.sigma(name),
        "seedRms": fit.seed_rms_3d_m, "rms": fit.residual_stats.rms_3d_m,
        "residuals": fit.residuals.iter().map(|residual| [
            residual.epoch_tdb_j2000_s, residual.elapsed_tt_s, residual.radial_m, residual.along_m, residual.cross_m,
        ]).collect::<Vec<_>>(),
    })
}

fn attitude_trajectories() -> Value {
    use gnss::{AttitudeTrajectory, GnssAttitudeLaw, OrbitAttitudeTrajectory as Trajectory};
    use sidereon_core::astro::frames::TdbEarthOrientationProvider;
    use sidereon_core::astro::propagator::{ForceModelKind, StatePropagator};
    use sidereon_core::astro::time::{j2000_seconds, timescale_offset_s, Instant, TimeScale};
    use sidereon_core::ephemeris::{BroadcastEphemeris, Sp3};
    use sidereon_core::orbit_scoring::{
        BroadcastOrbit, OrbitSource, OrbitTrajectory, PredictedSp3, TrajectorySample,
    };

    fn evaluate(trajectory: Trajectory<'_>, epoch: f64) -> Value {
        let sample = trajectory.sample(epoch).expect("attitude sample");
        let attitude = gnss::gnss_attitude(&GnssAttitudeLaw::OrbitNormal, &trajectory, epoch)
            .expect("attitude");
        let dyb = attitude.dyb.unwrap();
        json!({
            "epoch": epoch,
            "sample": [sample.position_m, sample.velocity_m_s, sample.sun_position_m],
            "angles": [attitude.yaw_rad, attitude.nominal_yaw_rad, attitude.beta_rad, attitude.orbit_angle_rad],
            "axes": [attitude.body_x, attitude.body_y, attitude.body_z, dyb.d, dyb.y, dyb.b],
        })
    }

    let sp3 = Sp3::parse(include_bytes!(
        "../../../fixtures/sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3"
    ))
    .unwrap();
    let nav = BroadcastEphemeris::from_nav(include_str!(
        "../../../fixtures/nav/ESBC00DNK_R_20201770000_01D_MN.rnx"
    ))
    .unwrap();
    let satellite = "G03".parse().unwrap();
    let earth_fixed_epoch = tdb_from_tt_j2000_seconds(
        j2000_seconds(2020, 6, 25, 0, 30, 45.0)
            + timescale_offset_s(TimeScale::Gpst, TimeScale::Tt).unwrap(),
    );
    let broadcast_satellite = "G01".parse().unwrap();
    let broadcast_gpst = j2000_seconds(2020, 6, 25, 4, 15, 0.0);
    let broadcast_epoch = tdb_from_tt_j2000_seconds(
        broadcast_gpst + timescale_offset_s(TimeScale::Gpst, TimeScale::Tt).unwrap(),
    );
    let tdb = TdbEarthOrientationProvider::new();
    let iers = Iers2010EarthOrientationProvider::new(
        [59025, 59026]
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
            .to_vec(),
    )
    .unwrap();
    let predicted = PredictedSp3::from_prediction_flags(&sp3).unwrap();
    let broadcast = BroadcastOrbit::new(&nav, Instant::from_nanos(TimeScale::Gpst, 0)).unwrap();
    let broadcast_state = broadcast
        .state_at_j2000_s(broadcast_satellite, broadcast_gpst)
        .unwrap();
    let origin = Instant::from_nanos(TimeScale::Tdb, 0);
    let nodes = OrbitTrajectory::new(
        "ITRF2020",
        origin,
        [
            TrajectorySample {
                satellite,
                epoch: origin,
                position_ecef_m: [20e6, 10e6, 2e6],
                clock_s: None,
            },
            TrajectorySample {
                satellite,
                epoch: Instant::from_nanos(TimeScale::Tdb, 43_200_000_000_000),
                position_ecef_m: [20e6, 10.02e6, 2e6],
                clock_s: None,
            },
        ],
    )
    .unwrap();

    let gravity = analytic::ZonalHarmonics::earth();
    let eh = analytic::EcksteinHechlerPropagator::from_mean_elements(
        0.0,
        analytic::CircularElements {
            a_km: 26560.0,
            ex: 0.0,
            ey: 0.0,
            i_rad: 0.4,
            raan_rad: 0.0,
            mean_latitude_argument_rad: 4.9,
        },
        &gravity,
    )
    .unwrap();
    let bl = analytic::BrouwerLyddanePropagator::from_mean_elements(
        0.0,
        analytic::KeplerianElements {
            a_km: 7000.0,
            e: 0.03,
            i_rad: 0.7,
            arg_perigee_rad: 0.2,
            raan_rad: 0.3,
            mean_anomaly_rad: 0.4,
        },
        &gravity,
        1e-17,
    )
    .unwrap();
    let de = DeEphemeris::from_bytes(include_bytes!("../../../fixtures/bodies/de440.440")).unwrap();
    let mut options = IntegratorOptions::default();
    options.dense_output = true;
    let propagator = StatePropagator::new(
        0.0,
        [7000.0, 1000.0, 200.0],
        [-1.0, 7.5, 0.8],
        ForceModelKind::TwoBody {
            mu_km3_s2: 398600.4418,
        },
        IntegratorKind::Dp54,
    )
    .with_options(options);
    let propagated = propagator.propagate_to(300.0).unwrap();
    let backward = propagator.propagate_to(-300.0).unwrap();
    let dense = propagated.dense.as_ref().unwrap();
    let final_state = propagated.final_state;
    json!({
        "sp3": evaluate(Trajectory::from_sp3(&sp3, satellite, &iers), earth_fixed_epoch),
        "predicted": evaluate(Trajectory::from_orbit_source(&predicted, satellite, &tdb), earth_fixed_epoch),
        "predictedIers": evaluate(Trajectory::from_orbit_source(&predicted, satellite, &iers), earth_fixed_epoch),
        "broadcast": evaluate(Trajectory::from_broadcast(&nav, broadcast_satellite, &tdb), broadcast_epoch),
        "broadcastIers": evaluate(Trajectory::from_broadcast(&nav, broadcast_satellite, &iers), broadcast_epoch),
        "broadcastEcef": {"epoch": broadcast_gpst, "position": broadcast_state.position_ecef_m, "clock": broadcast_state.clock_s},
        "broadcastSource": evaluate(Trajectory::from_orbit_source(&broadcast, broadcast_satellite, &tdb), broadcast_epoch),
        "sampled": evaluate(Trajectory::from_orbit_source(&nodes, satellite, &tdb).with_velocity_half_step_s(43200.0).unwrap(), 0.0),
        "ecksteinHechler": evaluate(Trajectory::from_eckstein_hechler(&eh), 0.0),
        "brouwerLyddane": evaluate(Trajectory::from_brouwer_lyddane(&bl), 600.0),
        "analyticDe": evaluate(Trajectory::from_brouwer_lyddane(&bl).with_de_ephemeris(&de), de.coverage_tdb_seconds().0),
        "dense": evaluate(Trajectory::from_dense_output(dense), 123.0),
        "denseCoverage": {
            "forward": dense.coverage_tdb_seconds().unwrap(),
            "backward": backward.dense.as_ref().unwrap().coverage_tdb_seconds().unwrap(),
        },
        "finalState": [final_state.position_array(), final_state.velocity_array()],
    })
}

fn earth_radiation_ephemeris() -> Value {
    use sidereon_core::astro::propagator::{ForceModelComponents, ForceModelKind, StatePropagator};

    let earth = forces::EarthRadiationPressure::new(1.2, 0.02)
        .unwrap()
        .with_moon_eclipse(false);
    let configuration =
        ForceModelKind::composite(ForceModelComponents::EMPTY.with_earth_radiation_pressure(earth));
    let states = StatePropagator::new(
        0.0,
        [7000.0, 0.0, 0.0],
        [0.0, 7.5, 0.0],
        configuration,
        IntegratorKind::Dp54,
    )
    .ephemeris(&[600.0])
    .unwrap();
    let final_state = &states[0];
    json!([final_state.position_array(), final_state.velocity_array()])
}

fn sp3_parameter_fit() -> Value {
    use sidereon_core::astro::propagator::{ForceModelComponents, ForceModelKind};
    use sidereon_core::ephemeris::Sp3;
    use sidereon_core::orbit_determination::{
        fit_sp3_ecef_orbit_parameters, OrbitParameterFitOptions,
    };

    let product = Sp3::parse(include_bytes!(
        "../../../fixtures/sp3/g02_ecef_two_epoch.sp3"
    ))
    .unwrap();
    let orientation = Iers2010EarthOrientationProvider::new(
        [61159, 61160, 61161]
            .map(|mjd_utc| Iers2010EopSample {
                mjd_utc,
                xp_arcsec: 0.0,
                yp_arcsec: 0.0,
                ut1_minus_utc_s: 0.0,
                lod_s: 0.0,
                dx_arcsec: 0.0,
                dy_arcsec: 0.0,
                pole_rates_rad_s: None,
            })
            .to_vec(),
    )
    .unwrap();
    let mut options = OrbitParameterFitOptions::default();
    options.force_model = ForceModelKind::composite(ForceModelComponents::EMPTY);
    let fit = fit_sp3_ecef_orbit_parameters(
        &product,
        "G02".parse().unwrap(),
        &orientation,
        None,
        &options,
    )
    .unwrap();
    json!({
        "epoch": fit.initial_state.epoch_tdb_seconds,
        "state": [fit.initial_state.position_array(), fit.initial_state.velocity_array()],
    })
}

fn additional_force_ephemeris() -> Value {
    use sidereon_core::astro::propagator::{ForceModelComponents, ForceModelKind, StatePropagator};
    use std::sync::Arc;

    let force = forces::EmpiricalRtnAcceleration::constant_terms()
        .with_term(
            forces::RtnAxis::Radial,
            forces::EmpiricalHarmonic::Constant,
            1e-7,
        )
        .unwrap();
    let propagator = StatePropagator::new(
        0.0,
        [7000.0, 0.0, 0.0],
        [0.0, 7.5, 0.0],
        ForceModelKind::composite(ForceModelComponents::EMPTY),
        IntegratorKind::Dp54,
    );
    let states = propagator
        .ephemeris_with_additional_forces(&[60.0], &[Arc::new(force)], &PropagationContext::new())
        .unwrap();
    json!([states[0].position_array(), states[0].velocity_array()])
}

fn fit_force_configuration() -> Value {
    use sidereon_core::astro::propagator::ForceModelKind;
    use sidereon_core::orbit_determination::OrbitParameterFitOptions;

    let state = CartesianState::new(0.0, [7000.0, 0.0, 0.0], [0.0, 7.5, 0.0]);
    let context = PropagationContext::new();
    let default = OrbitParameterFitOptions::default()
        .force_model
        .build()
        .unwrap();
    let explicit = ForceModelKind::two_body_j2().build().unwrap();
    json!({
        "default": default.acceleration(&state, &context).unwrap().as_slice(),
        "twoBodyJ2": explicit.acceleration(&state, &context).unwrap().as_slice(),
    })
}

fn gravity_accelerations() -> Value {
    use sidereon_core::astro::forces::ForceModel;

    let j2 = forces::J2Gravity::default();
    let context = PropagationContext::new();
    let probe = CartesianState::new(0.0, [7000.0, -1210.0, 1300.0], [0.0; 3]);
    let unused = CartesianState::new(0.0, [7000.0, 100.0, 200.0], [0.0; 3]);
    let distant = CartesianState::new(0.0, [1e60, 2e60, 3e60], [0.0; 3]);
    json!({
        "twoBody": forces::TwoBodyGravity::default().acceleration(&probe, &context).unwrap().as_slice(),
        "j2": j2.acceleration(&probe, &context).unwrap().as_slice(),
        "j2UnusedInputs": j2.acceleration(&unused, &context).unwrap().as_slice(),
        "j2Distant": j2.acceleration(&distant, &context).unwrap().as_slice(),
    })
}

fn main() {
    let document = json!({
        "solar": solar(), "de": de(), "eop": eop(), "attitude": attitude(),
        "analytic": analytic_orbits(), "radiation": radiation(), "variational": variational(),
        "parameterFit": parameter_fit(), "attitudeTrajectories": attitude_trajectories(),
        "earthRadiationEphemeris": earth_radiation_ephemeris(),
        "sp3ParameterFit": sp3_parameter_fit(),
        "additionalForceEphemeris": additional_force_ephemeris(),
        "fitForceConfiguration": fit_force_configuration(),
        "gravityAccelerations": gravity_accelerations(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&document).expect("serialize reference data")
    );
}
