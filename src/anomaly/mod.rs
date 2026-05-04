//! Statistical anomaly detection for crate compilation times.
//!
//! Uses a 2σ (two standard deviations) threshold to classify compilations
//! as normal, slower than expected, or faster than expected.
//!
//! All functions are pure (no I/O, no database access) and can be tested
//! without any infrastructure.

use std::time::Duration;

use crate::model::Baseline;

/// Classification result for a crate's compilation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyVerdict {
    /// Compilation time is within the expected range (mean ± threshold_sigma * std_dev).
    Normal,
    /// Compilation time exceeds the upper bound (mean + threshold_sigma * std_dev).
    Slower,
    /// Compilation time is below the lower bound (mean - threshold_sigma * std_dev).
    Faster,
    /// Insufficient historical data to make a determination.
    Unknown,
}

/// Classify a completed compilation against its historical baseline.
///
/// # Arguments
///
/// * `current` — The observed compilation duration.
/// * `baseline` — Historical statistics for this crate. `None` means no data.
/// * `threshold_sigma` — Number of standard deviations for the threshold (typically 2.0).
///
/// # Returns
///
/// - `Unknown` if `baseline` is `None`.
/// - `Slower` if `current > mean + threshold_sigma * std_dev`.
/// - `Faster` if `current < mean - threshold_sigma * std_dev`.
/// - `Normal` otherwise.
pub fn classify(
    current: Duration,
    baseline: Option<&Baseline>,
    threshold_sigma: f64,
) -> AnomalyVerdict {
    let baseline = match baseline {
        Some(b) => b,
        None => return AnomalyVerdict::Unknown,
    };

    let current_ms = current.as_secs_f64() * 1000.0;
    let mean_ms = baseline.mean.as_secs_f64() * 1000.0;
    let std_dev_ms = baseline.std_dev.as_secs_f64() * 1000.0;

    let upper = mean_ms + threshold_sigma * std_dev_ms;
    let lower = mean_ms - threshold_sigma * std_dev_ms;

    if current_ms > upper {
        AnomalyVerdict::Slower
    } else if current_ms < lower {
        AnomalyVerdict::Faster
    } else {
        AnomalyVerdict::Normal
    }
}

/// Classify an in-progress compilation based on elapsed time so far.
///
/// This is a one-sided check: we can only determine if the compilation
/// has already exceeded the upper threshold. We cannot say it's "faster"
/// until it actually finishes.
///
/// # Arguments
///
/// * `elapsed` — Time elapsed since compilation started.
/// * `baseline` — Historical statistics for this crate. `None` means no data.
/// * `threshold_sigma` — Number of standard deviations for the threshold.
///
/// # Returns
///
/// - `Unknown` if `baseline` is `None`.
/// - `Slower` if `elapsed` already exceeds `mean + threshold_sigma * std_dev`.
/// - `Normal` otherwise (we can't determine "faster" for in-progress compilations).
pub fn classify_in_progress(
    elapsed: Duration,
    baseline: Option<&Baseline>,
    threshold_sigma: f64,
) -> AnomalyVerdict {
    let baseline = match baseline {
        Some(b) => b,
        None => return AnomalyVerdict::Unknown,
    };

    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let mean_ms = baseline.mean.as_secs_f64() * 1000.0;
    let std_dev_ms = baseline.std_dev.as_secs_f64() * 1000.0;

    let upper = mean_ms + threshold_sigma * std_dev_ms;

    if elapsed_ms > upper {
        AnomalyVerdict::Slower
    } else {
        AnomalyVerdict::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Baseline, CrateId};

    fn make_baseline(mean_ms: u64, std_dev_ms: u64) -> Baseline {
        Baseline {
            crate_id: CrateId {
                name: "test-crate".to_string(),
                version: None,
            },
            sample_count: 10,
            mean: Duration::from_millis(mean_ms),
            std_dev: Duration::from_millis(std_dev_ms),
            min: Duration::from_millis(mean_ms.saturating_sub(std_dev_ms * 3)),
            max: Duration::from_millis(mean_ms + std_dev_ms * 3),
        }
    }

    #[test]
    fn test_classify_normal() {
        // mean=5000ms, std_dev=500ms, threshold=2σ
        // upper = 6000ms, lower = 4000ms
        let baseline = make_baseline(5000, 500);
        let current = Duration::from_millis(5200); // within range

        assert_eq!(
            classify(current, Some(&baseline), 2.0),
            AnomalyVerdict::Normal
        );
    }

    #[test]
    fn test_classify_slower() {
        // mean=5000ms, std_dev=500ms, threshold=2σ
        // upper = 6000ms
        let baseline = make_baseline(5000, 500);
        let current = Duration::from_millis(6500); // exceeds upper bound

        assert_eq!(
            classify(current, Some(&baseline), 2.0),
            AnomalyVerdict::Slower
        );
    }

    #[test]
    fn test_classify_faster() {
        // mean=5000ms, std_dev=500ms, threshold=2σ
        // lower = 4000ms
        let baseline = make_baseline(5000, 500);
        let current = Duration::from_millis(3500); // below lower bound

        assert_eq!(
            classify(current, Some(&baseline), 2.0),
            AnomalyVerdict::Faster
        );
    }

    #[test]
    fn test_classify_unknown_no_baseline() {
        let current = Duration::from_millis(5000);

        assert_eq!(classify(current, None, 2.0), AnomalyVerdict::Unknown);
    }

    #[test]
    fn test_classify_in_progress_normal() {
        let baseline = make_baseline(5000, 500);
        let elapsed = Duration::from_millis(4000); // still within range

        assert_eq!(
            classify_in_progress(elapsed, Some(&baseline), 2.0),
            AnomalyVerdict::Normal
        );
    }

    #[test]
    fn test_classify_in_progress_slower() {
        let baseline = make_baseline(5000, 500);
        let elapsed = Duration::from_millis(6500); // already exceeded upper

        assert_eq!(
            classify_in_progress(elapsed, Some(&baseline), 2.0),
            AnomalyVerdict::Slower
        );
    }

    #[test]
    fn test_classify_in_progress_unknown() {
        let elapsed = Duration::from_millis(5000);

        assert_eq!(
            classify_in_progress(elapsed, None, 2.0),
            AnomalyVerdict::Unknown
        );
    }

    #[test]
    fn test_classify_at_exact_boundary() {
        // Exactly at the upper boundary should be Normal (not Slower).
        let baseline = make_baseline(5000, 500);
        let current = Duration::from_millis(6000); // exactly at mean + 2σ

        assert_eq!(
            classify(current, Some(&baseline), 2.0),
            AnomalyVerdict::Normal
        );
    }
}
