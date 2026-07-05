//! # Time-Series Ring Buffer
//!
//! Fixed-capacity ring buffer with timestamped entries, designed for:
//! - Feeding Ratatui `Chart` widgets (values_for_chart)
//! - Sliding-window anomaly detection (linear_regression)
//! - Memory-bounded storage (oldest entries evicted automatically)

use std::collections::VecDeque;
use std::time::Instant;

/// A timestamped ring buffer of `T` values.
///
/// Stores up to `capacity` entries. When full, the oldest entry is evicted
/// on the next `push`. All reads are O(1) amortised; linear regression is O(n).
#[derive(Debug, Clone)]
pub struct TimeSeries<T> {
    buf: VecDeque<(Instant, T)>,
    capacity: usize,
}

impl<T: Clone> TimeSeries<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "TimeSeries capacity must be > 0");
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a value with the current timestamp.
    pub fn push(&mut self, value: T) {
        self.push_at(Instant::now(), value);
    }

    /// Push a value with an explicit timestamp.
    pub fn push_at(&mut self, at: Instant, value: T) {
        if self.buf.len() >= self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back((at, value));
    }

    /// Number of entries currently stored.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the most recent entry.
    pub fn latest(&self) -> Option<&(Instant, T)> {
        self.buf.back()
    }

    /// Get the oldest entry.
    pub fn oldest(&self) -> Option<&(Instant, T)> {
        self.buf.front()
    }

    /// Iterate over all entries oldest-first.
    pub fn iter(&self) -> impl Iterator<Item = &(Instant, T)> {
        self.buf.iter()
    }

    /// Extract the last `n` values (without timestamps).
    pub fn last_n_values(&self, n: usize) -> Vec<&T> {
        let skip = self.buf.len().saturating_sub(n);
        self.buf.iter().skip(skip).map(|(_, v)| v).collect()
    }

    /// Produce (seconds_ago, extracted_value) pairs for Ratatui Chart widgets.
    ///
    /// `extractor` pulls a f64 from T (e.g., `|s| s.heap.used as f64`).
    /// The x-axis is seconds before `now`, so the rightmost point is ~0.
    pub fn values_for_chart(&self, extractor: impl Fn(&T) -> f64) -> Vec<(f64, f64)> {
        if self.buf.is_empty() {
            return vec![];
        }
        let now = Instant::now();
        self.buf
            .iter()
            .map(|(t, v)| {
                let secs_ago = now.duration_since(*t).as_secs_f64();
                (secs_ago, extractor(v))
            })
            .collect()
    }

    /// Simple linear regression on extracted values over time.
    ///
    /// Returns `Some((slope, intercept, r_squared))` if there are >= 3 data points.
    /// - `slope`: units of extracted value per second
    /// - `r_squared`: goodness of fit (0.0–1.0)
    ///
    /// Used by anomaly detectors (e.g., memory leak = positive slope on post-GC old-gen).
    pub fn linear_regression(&self, extractor: impl Fn(&T) -> f64) -> Option<RegressionResult> {
        let n = self.buf.len();
        if n < 3 {
            return None;
        }

        let origin = self.buf.front()?.0;

        let mut sum_x = 0.0_f64;
        let mut sum_y = 0.0_f64;
        let mut sum_xx = 0.0_f64;
        let mut sum_xy = 0.0_f64;
        let nf = n as f64;

        for (t, v) in &self.buf {
            let x = t.duration_since(origin).as_secs_f64();
            let y = extractor(v);
            sum_x += x;
            sum_y += y;
            sum_xx += x * x;
            sum_xy += x * y;
        }

        let denom = nf * sum_xx - sum_x * sum_x;
        if denom.abs() < f64::EPSILON {
            return None;
        }

        let slope = (nf * sum_xy - sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * sum_x) / nf;

        // R² (coefficient of determination)
        let ss_res: f64 = self
            .buf
            .iter()
            .map(|(t, v)| {
                let x = t.duration_since(origin).as_secs_f64();
                let y = extractor(v);
                let predicted = slope * x + intercept;
                (y - predicted).powi(2)
            })
            .sum();

        let mean_y = sum_y / nf;
        let ss_tot: f64 = self
            .buf
            .iter()
            .map(|(_, v)| {
                let y = extractor(v);
                (y - mean_y).powi(2)
            })
            .sum();

        let r_squared = if ss_tot.abs() < f64::EPSILON {
            1.0 // All y values identical — perfect fit
        } else {
            1.0 - (ss_res / ss_tot)
        };

        Some(RegressionResult {
            slope,
            intercept,
            r_squared,
            sample_count: n,
            window_secs: self.buf.back()?.0.duration_since(origin).as_secs_f64(),
        })
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

/// Result of a linear regression on time-series data.
#[derive(Debug, Clone)]
pub struct RegressionResult {
    /// Rate of change per second.
    pub slope: f64,
    /// Y-intercept at the start of the window.
    pub intercept: f64,
    /// Goodness of fit (0.0–1.0). >0.7 is meaningful for anomaly detection.
    pub r_squared: f64,
    /// Number of data points used.
    pub sample_count: usize,
    /// Duration of the observation window in seconds.
    pub window_secs: f64,
}

impl RegressionResult {
    /// Slope expressed per hour instead of per second.
    pub fn slope_per_hour(&self) -> f64 {
        self.slope * 3600.0
    }

    /// Extrapolate: how many seconds until the value reaches `target`?
    /// Returns None if slope is zero/negative or already past target.
    pub fn time_to_reach(&self, current: f64, target: f64) -> Option<f64> {
        if self.slope <= 0.0 || current >= target {
            return None;
        }
        Some((target - current) / self.slope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn push_and_evict() {
        let mut ts = TimeSeries::new(3);
        ts.push(10);
        ts.push(20);
        ts.push(30);
        assert_eq!(ts.len(), 3);

        ts.push(40);
        assert_eq!(ts.len(), 3);
        assert_eq!(ts.oldest().unwrap().1, 20);
        assert_eq!(ts.latest().unwrap().1, 40);
    }

    #[test]
    fn linear_regression_positive_slope() {
        let mut ts = TimeSeries::new(100);
        let start = Instant::now();

        // y = 10x (10 units per second)
        for i in 0..10 {
            ts.push_at(start + Duration::from_secs(i), (i * 10) as f64);
        }

        let reg = ts.linear_regression(|v| *v).unwrap();
        assert!((reg.slope - 10.0).abs() < 0.01);
        assert!(reg.r_squared > 0.99);
        assert_eq!(reg.sample_count, 10);
    }

    #[test]
    fn linear_regression_too_few_points() {
        let mut ts = TimeSeries::new(10);
        ts.push(1.0);
        ts.push(2.0);
        assert!(ts.linear_regression(|v| *v).is_none());
    }

    #[test]
    fn time_to_reach_extrapolation() {
        let result = RegressionResult {
            slope: 100.0, // 100 bytes/sec
            intercept: 0.0,
            r_squared: 0.95,
            sample_count: 20,
            window_secs: 300.0,
        };

        // At current=500, how long to reach 1000?
        let secs = result.time_to_reach(500.0, 1000.0).unwrap();
        assert!((secs - 5.0).abs() < 0.01);
    }

    #[test]
    fn last_n_values() {
        let mut ts = TimeSeries::new(10);
        for i in 0..5 {
            ts.push(i);
        }
        let last_3: Vec<_> = ts.last_n_values(3);
        assert_eq!(last_3, vec![&2, &3, &4]);
    }
}
