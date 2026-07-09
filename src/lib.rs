//! WebAssembly / JavaScript interface over the sidereon GNSS + astrodynamics
//! engine.
//!
//! This crate is a thin interface: it normalizes JS input, marshals it into the
//! `sidereon` / `sidereon-core` types, calls the reference entry point, and
//! packages the result for JS. It contains no modeling logic of its own; the
//! numbers it returns are what `sidereon-core` produces.
//!
//! Only the serial engine paths are used (`solve_spp`, `propagate_teme_arc`,
//! `look_angle_arc`, `predict_batch`, `solve_data_problem`). The rayon
//! `*_parallel` batch variants are never called, so no thread pool is ever
//! spawned; the data-driven leave-one-out sweep, whose core entry point
//! (`solve_data_problem_drop_one`) fans across rayon, is driven serially here
//! one masked row at a time. rayon links in (it is an unconditional core
//! dependency) but its runtime is never entered under wasm32.

mod almanac;
mod anomaly;
mod antex;
mod araim;
mod atmosphere;
mod bias;
mod bodies;
mod broadcast_comparison;
mod cdm;
mod clock_stability;
mod conjunction;
mod constellation;
mod covariance;
mod coverage;
mod crinex;
mod dgnss;
mod dop;
mod doppler;
mod elements;
mod emission_media;
mod equinoctial;
mod error;
mod error_metrics;
mod estimation;
mod events;
mod force_model_input;
mod forces;
mod frame_catalog;
mod frames;
mod fusion;
mod geodesic;
mod geodetic_time_series;
mod geofence;
mod geoid;
mod geometry_quality;
mod gnss;
mod ils;
mod iod;
mod ionex;
mod ionosphere;
mod lambert;
mod least_squares;
mod lnav;
mod marshal;
mod moving_baseline;
mod nmea;
mod normality;
mod ntrip;
mod observables;
mod observation;
mod oem;
mod omm;
mod opm;
mod orbit_determination;
mod ppp;
mod ppp_corrections;
mod precise_samples;
mod propagation;
mod qc;
mod raim;
mod reduced_orbit;
mod relative;
mod reliability;
mod rf;
mod rinex_clock;
mod rinex_nav;
mod rinex_obs;
mod rinex_qc;
mod rtcm;
mod rtk;
mod rtk_arc;
mod sbas;
mod sbas_pl;
mod scenario;
mod sgp4;
mod sidereal;
mod signal_analysis;
mod sky;
mod source_localization;
mod sp3;
mod sp3_merge;
mod space_weather;
mod spk;
mod spp;
mod ssr;
mod staleness;
mod static_positioning;
mod tca;
mod tdm;
mod terrain;
mod terrain_store;
mod tides;
mod trls;
mod tropo;

pub use almanac::{
    lunar_solar_eclipses, lunar_solar_eclipses_spk, meridian_transits, meridian_transits_spk,
    moon_phases, moon_phases_spk, planetary_events, seasons, seasons_spk,
};
pub use anomaly::{
    eccentric_to_mean, eccentric_to_true, mean_to_eccentric, mean_to_true, propagate_kepler,
    solve_kepler, true_to_eccentric, true_to_mean,
};
pub use antex::{load_antex, Antenna, Antex, AntexDateTime};
pub use araim::{araim, araim_fault_modes, araim_lpv_200_allocation};
pub use atmosphere::{atmosphere_density, AtmosphereDensity};
pub use bias::{
    load_bias_sinex, load_bias_sinex_lossy, load_code_dcb, load_code_dcb_lossy, BiasSet,
};
pub use bodies::{sun_moon_ecef_batch, sun_moon_eci, SunMoon};
pub use cdm::{parse_cdm_kvn, parse_cdm_xml, Cdm, CdmObject};
pub use clock_stability::{
    allan_deviation, allan_deviation_power_law_slope, allan_variance_power_law_tau_exponent,
    compute_allan_deviations, fit_power_law_noise, hadamard_deviation, modified_adev,
    modified_allan_deviation_power_law_slope, overlapping_adev, time_deviation, PowerLawNoiseType,
};
pub use conjunction::{
    collision_probability, covariance_is_positive_semidefinite, covariance_is_symmetric,
    encounter_frame, encounter_plane_covariance, rtn_to_eci_covariance, CollisionProbability,
    ConjunctionState, EncounterFrame,
};
pub use constellation::{
    changed_js, diff_js, from_celestrak_json, from_celestrak_json_lenient, glonass_fdma_channel_js,
    gnss_sp3_id_js, is_valid_js, merge_navcen_js, parse_navcen, to_csv_js,
    validate_against_sp3_ids_js, validate_js,
};
pub use covariance::{
    propagate_covariance, transport_covariance_js, CovarianceEphemeris, CovarianceFrame,
    CovarianceTransportResult,
};
pub use coverage::{coverage_look_angles, CoverageGrid};
pub use crinex::{decode_crinex, decode_crinex_lines, encode_crinex, load_crinex};
pub use dgnss::{dgnss_apply, AppliedCorrections, CorrectionEntry, DgnssSolution};
pub use dop::{
    dop_with_convention_js, error_ellipse_2, gnss_dop, gnss_dop_at_epoch, gnss_dop_series,
    gnss_dop_series_window, gnss_passes, gnss_visibility_series, gnss_visible, Dop, DopGeometry,
    DopSeries, DopSeriesSample, ErrorEllipse2, GnssPass, GnssVisibilityCount, GnssVisibleSatellite,
    Wgs84Geodetic,
};
pub use doppler::{doppler_range_rate, doppler_shift_js, DopplerShift};
pub use elements::{coe2rv, rv2coe};
pub use emission_media::{emission_media_status_label, EmissionMediaBatch, EmissionMediaStatus};
pub use equinoctial::{
    coe2eq, coe2mee, eq2coe, eq2rv, mee2coe, mee2rv, rv2eq, rv2mee, RetrogradeFactor,
};
pub use error_metrics::{
    error_ellipse_from_enu_m2, horizontal_radius_at, metrics_from_ecef_covariance_m2,
    metrics_from_enu_covariance_m2, metrics_from_kinematic_solution,
    metrics_from_position_covariance, spherical_radius_at, vertical_radius_at, ErrorEllipse,
    PercentileRadius, PositionErrorMetrics,
};
pub use estimation::{
    alpha_beta_apply_measurement, alpha_beta_filter_step, alpha_beta_predict,
    alpha_beta_steady_state_gains, cfar_ca_false_alarm_probability, cfar_ca_multiplier_from_pfa,
    cfar_ca_pfa_from_multiplier, cfar_ca_threshold, ewma_update, ewma_update_power_of_two,
    kalman_cv_steady_state_gains, mad_gaussian_consistency, mad_spread, nis, nis_expected_value,
    nis_gate, nis_gate_threshold, normalized_innovation, smooth_track_rts, SmoothedTrack,
    TrackFilter, TrackFilterConfig, TrackRtsHistory, TrackRtsHistoryBuilder,
};
pub use events::{
    angular_separation, angular_separation_coords, beta_angle, beta_angle_from_state,
    earth_angular_radius, eclipse_status, moon_angle, phase_angle, position_angle, shadow_fraction,
    shadow_fraction_with_model, sun_angle, sun_elevation, EarthShadowModel,
};
pub use forces::{
    estimate_decay, force_j2_acceleration, force_twobody_acceleration, DragForce, SpaceWeather,
};
pub use frame_catalog::{
    frame_catalog, frame_catalog_entry, frame_catalog_propagate_position, frame_catalog_transform,
    frame_catalog_transform_from_epoch, terrestrial_frame_label, HelmertParameters, HelmertRates,
    HelmertTransform, TerrestrialFrame,
};
pub use frames::{
    civil_to_j2000_seconds, ecef_to_geodetic, gcrs_to_itrs, geodetic_to_ecef, gps_utc_offset_s,
    itrs_to_gcrs, j2000_seconds_to_civil, leap_second_table_info, leap_seconds, leap_seconds_batch,
    split_jd_to_j2000_seconds, tai_utc_offset_s, teme_to_gcrs, time_scale_abbrev,
    timescale_offset_at_s_js, timescale_offset_s_js, ut1_coverage_info, CivilDateTime, FrameStates,
    GnssWeekTow, Instant, JulianDate, LeapSecondTable, TimeScale, Ut1Coverage,
};
pub use fusion::{
    fusion_state_bytes_round_trip, smooth_fusion_rts, velocity_match_outage, FusionRtsHistory,
    FusionRtsHistoryBuilder, GnssInsFilter, SmoothedFusionTrajectory,
};
pub use geodesic::{geodesic_direct, geodesic_error_label, geodesic_inverse, GeodesicError};
pub use geodetic_time_series::{detect_steps, fit_trajectory, network_field, velocity_midas};
pub use geofence::{
    geofence_containment_probability, geofence_contains, geofence_crossing_kind_label,
    geofence_crossing_probability, geofence_error_label, geofence_from_vertices,
    geofence_from_vertices_3d, geofence_probability_method_label, Geofence, GeofenceCrossingKind,
    GeofenceError, GeofenceProbabilityMethod,
};
pub use geoid::{
    egm96_ellipsoidal_height_m, egm96_orthometric_height_m, egm96_undulation,
    egm96_undulations_deg, egm96_undulations_rad, ellipsoidal_height_m, geoid_undulation,
    geoid_undulations_deg, geoid_undulations_rad, orthometric_height_m, Egm2008GridSpacing,
    GeoidGrid,
};
pub use geometry_quality::{observability_tier_label, GeometryQuality, ObservabilityTier};
pub use gnss::{carrier_band_name, gnss_system_label, gnss_system_letter, CarrierBand, GnssSystem};
pub use ils::{bounded_ils_search_js, lambda_ils_search_js};
pub use iod::{iod_gauss_angles, iod_gibbs, iod_herrick_gibbs, IodState, IodVelocity};
pub use ionex::{ionex_from_node_samples, ionex_from_samples, load_ionex, Ionex};
pub use ionosphere::{
    galileo_nequick_delay, klobuchar_delay, nequick_g_delay_m_js, nequick_g_stec_tecu_js,
};
pub use lambert::{lambert_battin, LambertTransfer};
pub use least_squares::{covariance_from_jacobian, hessian_trace, normal_covariance};
pub use lnav::{
    lnav_decode, lnav_encode, lnav_parity, lnav_parity_valid, lnav_subframe_id, lnav_tow,
    LnavDecoded, LnavSubframes,
};
pub use moving_baseline::{solve_moving_baseline, MovingBaselineSolution};
pub use nmea::{nmea_epochs, nmea_write_gga, parse_nmea, NmeaAccumulator, NmeaParseResult};
pub use normality::{jarque_bera, kurtosis, moments, shapiro_wilk, skewness};
pub use ntrip::{
    ntrip_request_bytes, parse_ntrip_sourcetable, NtripClientMachine, NtripState, NtripVersion,
};
pub use observables::{
    acquire, ca_chip, ca_code, carrier_frequency_hz, coherent_loss, coherent_loss_db, correlate,
    default_pair, default_spp_frequency_hz_js, detect_cycle_slips, doppler_to_range_rate, gamma,
    geometry_free, glonass_g1_frequency_hz_js, ionosphere_free, ionosphere_free_phase_cycles,
    ionosphere_free_phase_m, ionosphere_free_pseudoranges, melbourne_wubbena, narrow_lane_code,
    noise_amplification, observables_broadcast, observables_sp3, phase_meters,
    predict_batch_broadcast, predict_batch_sp3, pseudorange_variance, range_rate_to_doppler,
    replica, rinex_band_frequency_hz_js, rinex_band_wavelength_m_js, sigmas, slip_reason_label,
    smooth_code, smooth_iono_free_code, snr_post_db, solve_velocity, solve_velocity_broadcast,
    wavelength_m_js, weight_vector, wide_lane_cycles, wide_lane_wavelength, AcquisitionGrid,
    AcquisitionResult, CarrierPair, CorrelationResult, IonoFreePseudorangeResult,
    IonoFreeSmoothResult, PredictBatch, PredictedObservables, PseudorangeDropReason, RaimWeights,
    SatelliteVector, SlipReason, SlipResult, SmoothCodeResult, VelocitySolution,
};
pub use observation::{
    observe, observe_barycentric_state, observe_spk_body, parallactic_angle_deg,
    satellite_visual_magnitude, sub_observer_point, sub_solar_point, terminator_latitude_deg,
};
pub use oem::{
    parse_oem_kvn, parse_oem_xml, Oem, OemCovariance, OemMetadata, OemSegment, OemState,
};
pub use omm::{parse_omm_json, parse_omm_kvn, parse_omm_xml, Omm, OmmEpoch};
pub use opm::{
    parse_opm_kvn, parse_opm_xml, Opm, OpmCovariance, OpmKeplerian, OpmManeuver, OpmMetadata,
    OpmSpacecraft, OpmState,
};
pub use orbit_determination::{
    fit_all_sp3_ecef_precise_orbits, fit_precise_ephemeris_sample_orbit,
    fit_sp3_ecef_precise_orbit, fit_sp3_ecef_precise_orbits, fit_sp3_precise_orbit,
};
pub use ppp::{
    solve_ppp_auto_init_fixed_js, solve_ppp_auto_init_float_js, solve_ppp_fixed, solve_ppp_float,
    PppFixedSolution, PppFloatSolution,
};
pub use ppp_corrections::ppp_corrections;
pub use ppp_corrections::ppp_corrections_with_code_bias;
pub use precise_samples::{
    observable_state_missing_position_ecef_m, precise_ephemeris_samples_from_samples,
    sample_broadcast_ephemeris, sample_sp3_ephemeris, sp3_precise_ephemeris_samples,
    PreciseEphemerisInterpolant, PreciseEphemerisSampleSource,
};
pub use propagation::{propagate_state, Ephemeris};
pub use qc::FdeSolution;
pub use raim::{raim, raim_fde_design_js};
pub use reduced_orbit::{
    fit_piecewise_reduced_orbit, fit_piecewise_reduced_orbit_sp3, fit_piecewise_reduced_orbit_tle,
    fit_reduced_orbit, fit_reduced_orbit_sp3, fit_reduced_orbit_tle, PiecewiseOrbit,
    PiecewiseOrbitSourceFit, ReducedOrbit, ReducedOrbitDrift, ReducedOrbitSourceFit,
    ReducedOrbitState,
};
pub use relative::{
    cw_propagate, cw_stm, lvlh_rotation, mean_motion_circular, mean_motion_from_state,
    relative_state, ric_rotation, rsw_rotation, rtn_rotation,
};
pub use reliability::{reliability_araim, reliability_design, wtest_noncentrality};
pub use rf::{cn0, dish_gain, eirp, fspl, wavelength, LinkBudget};
pub use rinex_clock::{
    load_rinex_clock, load_rinex_clock_lossy, parse_rinex_clock, parse_rinex_clock_lossy,
    ClockEpoch, ClockSeries, RinexClock,
};
pub use rinex_nav::{
    cnav_ura_nominal_m, load_rinex_nav, parse_rinex_glonass_records, parse_rinex_iono_corrections,
    parse_rinex_leap_seconds, parse_rinex_nav, parse_rinex_nav_records, BroadcastDelayTerm,
    BroadcastEphemeris, BroadcastEvaluation, BroadcastGroupDelaysJs, BroadcastRecordJs,
    BroadcastStoreEvaluation, ClockPolynomialJs, CnavParametersJs, CnavSignal, GlonassRecordJs,
    IonoCorrectionsJs, KeplerianElementsJs, KlobucharAlphaBetaJs, NavMessage,
};
pub use rinex_obs::{
    load_rinex_obs, observation_kind_label, parse_rinex_obs, CarrierPhaseSeries, ObsEpoch,
    ObsEpochTime, ObsHeader, ObsPhaseShift, ObservationFilter, ObservationKind,
    ObservationValueSeries, PseudorangeSeries, RinexObs, SignalPolicy,
};
pub use rinex_qc::{
    lint_rinex_nav, lint_rinex_obs, observation_qc, repair_rinex_nav, repair_rinex_obs,
    ObservationQcReport, RinexNavRepair, RinexObsRepair,
};
pub use rtcm::{
    decode_rtcm, decode_rtcm_frame, decode_rtcm_message, decode_rtcm_stream, encode_rtcm,
    encode_rtcm_frame, rtcm_derive_lli, rtcm_lli_bits, rtcm_message_number,
    rtcm_minimum_lock_time_ms, rtcm_msm_epoch_dt_ms, rtcm_msm_signal_rinex_code, FrameScanner,
    RtcmLockTimeTracker,
};
pub use rtk::{solve_rtk_fixed, solve_rtk_float, RtkFixedSolution, RtkFloatSolution};
pub use rtk_arc::{
    build_dual_frequency_rinex_rtk_arc_js, build_rinex_rtk_arc_js, fix_wide_lane_rtk_arc_js,
    prepare_ionosphere_free_rtk_arc_js, solve_rtk_arc_js, solve_static_reference_station_rinex_js,
    solve_static_rinex_rtk_baseline_js, solve_static_rtk_arc_js,
    solve_wide_lane_fixed_rinex_rtk_baseline_js,
};
pub use sbas::{
    decode_sbas_message, sat_to_sbas_prn, sbas_corrected_state, sbas_prn_to_sat, solve_spp_sbas,
    SbasCorrectionStore,
};
pub use sbas_pl::{
    sbas_pl_error_label, sbas_protection_levels, AirborneModel, DegradationParams, SbasErrorModel,
    SbasKMultipliers, SbasPlError, SbasProtection, SbasSisError,
};
pub use scenario::{
    default_scenario_seed_hex, scenario_engine_version, scenario_schema_version, simulate_scenario,
    simulate_scenario_bytes, simulate_scenario_json, simulate_scenario_json_bytes,
};
pub use sgp4::{
    fit_tle, parse_tle_file, propagate_batch, visible_from_satellites_js, ChecksumWarning,
    Constellation, DecayLatch, FleetPass, FleetPropagation, GroundStation, GroundTrack, LookAngles,
    NamedTle, ParsedTleFile, SatellitePass, Tle, TleFit, TlePropagation, VisibilitySeries,
    VisibleSatellite,
};
pub use sidereal::{orbit_repeat_lag, periodicity_strength, repeat_period, sidereal_filter};
pub use signal_analysis::{
    effective_cn0_degradation, spectral_separation_coefficient_db_hz,
    spectral_separation_coefficient_hz, DllProcessing, SignalAnalysisModulation,
};
pub use sky::{
    find_moon_elevation_crossings, find_moon_transits, moon_az_el, moon_elevation_deg,
    moon_illumination, sun_az_el, MoonElevationCrossing, MoonTransit,
};
pub use source_localization::{
    chan_ho_initial_guess, locate_source, source_crlb, source_dop, source_solve_mode_tdoa,
    source_solve_mode_toa,
};
pub use sp3::{
    load_sp3, open_precise_interpolant_artifact, precise_interpolant_artifact_checksum64,
    precise_interpolant_artifact_error_label, PreciseInterpolantArtifact,
    PreciseInterpolantArtifactError, Sp3, Sp3ClockReferenceOffset, Sp3Interpolation, Sp3State,
};
pub use sp3_merge::{
    merge_sp3, Sp3FrameReconciliationReport, Sp3MergeFlag, Sp3MergeReport, Sp3MergeResult,
};
pub use space_weather::{
    estimate_decay_with_space_weather, parse_space_weather, parse_space_weather_csv,
    parse_space_weather_txt, SpaceWeatherTable,
};
pub use spk::{Spk, SpkSegment, SpkState};
pub use spp::{SppBatchSolution, SppDopplerSolution, SppSolution};
pub use ssr::{decode_ssr, ssr_corrected_state, ssr_source_label, SsrCorrectionStore, SsrSource};
pub use staleness::{
    select_ionex_js, select_ionex_over_range_js, select_sp3_js, select_sp3_over_range_js,
    solve_with_fallback_js, IonexSelection, SourcedSolution, Sp3Selection,
};
pub use static_positioning::{
    solve_static, static_influence_status_label, StaticInfluenceStatus, StaticSolution,
};
pub use tca::{
    find_tca_candidates, find_tca_conjunctions, screen_tca_candidates, screen_tca_conjunctions,
};
pub use tdm::{
    parse_tdm_kvn, Tdm, TdmDataRecord, TdmDataSection, TdmField, TdmMetadata, TdmParticipant,
    TdmPath, TdmScalar, TdmSegment,
};
pub use terrain::DtedTerrain;
pub use terrain_store::{
    dted_tree_to_mmap_store, terrain_store_checksum64, write_dted_tree_to_mmap_store,
    Egm96FifteenMinuteGeoid, EllipsoidalHeightM, MmapTerrain, OrthometricHeightM,
    TerrainDatumError, TerrainGeoidModel, TerrainStoreError, TerrainStoreTileIndex, VerticalDatum,
};
pub use tides::{ocean_tide_loading_js, solid_earth_pole_tide_js, solid_earth_tide_js};
pub use trls::{
    least_squares, least_squares_drop_one, LeastSquaresDropOneReport, LeastSquaresResult,
};
pub use tropo::{
    tropo_mapping_factors, tropo_slant_delay, tropo_zenith_delay, MappingFactors, ZenithDelay,
};
