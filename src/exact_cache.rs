//! Browser exact-cache single-flight decision kernel.
//!
//! IndexedDB owns serialized storage transactions in `exact-cache.js`. This
//! wrapper keeps progress, liveness, takeover, and timeout decisions in the
//! target-neutral `sidereon-core` state machine.

use std::time::Duration;

use serde::Serialize;
use sidereon_core::exact_cache::{
    ExactCacheSingleFlightDecision as CoreDecision, ExactCacheSingleFlightOptions as CoreOptions,
    ExactCacheSingleFlightWait as CoreWait,
};
use wasm_bindgen::prelude::*;

use crate::error::{engine_error, range_error};

#[derive(Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum WaitDecision {
    Wait {
        #[serde(rename = "delayMs")]
        delay_ms: f64,
    },
    Takeover,
    Timeout,
}

/// Stateful adapter around the core's target-neutral single-flight waiter.
#[wasm_bindgen]
pub struct ExactCacheSingleFlightWait {
    inner: CoreWait,
    options: CoreOptions,
}

#[wasm_bindgen]
impl ExactCacheSingleFlightWait {
    /// Start one bounded wait at a browser-monotonic timestamp in milliseconds.
    #[wasm_bindgen(constructor)]
    pub fn new(
        started_ms: f64,
        poll_interval_ms: f64,
        heartbeat_interval_ms: f64,
        liveness_timeout_ms: f64,
        wait_timeout_ms: f64,
    ) -> Result<ExactCacheSingleFlightWait, JsValue> {
        let started = duration_ms(started_ms, "startedMs")?;
        let options = CoreOptions {
            poll_interval: duration_ms(poll_interval_ms, "pollIntervalMs")?,
            heartbeat_interval: duration_ms(heartbeat_interval_ms, "heartbeatIntervalMs")?,
            liveness_timeout: duration_ms(liveness_timeout_ms, "livenessTimeoutMs")?,
            wait_timeout: duration_ms(wait_timeout_ms, "waitTimeoutMs")?,
        };

        // `observe` owns option validation in the core. Seeding with an empty
        // opaque revision is harmless: the first browser observation differs
        // and resets the no-progress clock to its actual observation time.
        let mut inner = CoreWait::new(started);
        inner.observe(started, &[], options).map_err(engine_error)?;
        Ok(Self { inner, options })
    }

    /// Observe an opaque IndexedDB owner/heartbeat revision.
    pub fn observe(&mut self, now_ms: f64, revision: &[u8]) -> Result<JsValue, JsValue> {
        let now = duration_ms(now_ms, "nowMs")?;
        let decision = match self
            .inner
            .observe(now, revision, self.options)
            .map_err(engine_error)?
        {
            CoreDecision::Wait(delay) => WaitDecision::Wait {
                delay_ms: delay.as_secs_f64() * 1_000.0,
            },
            CoreDecision::Takeover => WaitDecision::Takeover,
            CoreDecision::Timeout => WaitDecision::Timeout,
        };
        serde_wasm_bindgen::to_value(&decision).map_err(engine_error)
    }
}

fn duration_ms(value: f64, field: &str) -> Result<Duration, JsValue> {
    if !value.is_finite() || value < 0.0 {
        return Err(range_error(&format!(
            "{field} must be a finite, non-negative number"
        )));
    }
    Duration::try_from_secs_f64(value / 1_000.0)
        .map_err(|_| range_error(&format!("{field} is outside the supported duration range")))
}
