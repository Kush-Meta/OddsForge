//! Gradient Boosted Decision Stumps
//!
//! 150 additive decision stumps (depth-1 trees), each fit to the
//! negative gradient of binary log-loss on the residuals.
//! Final prediction: sigmoid(Σ lr * stump_i(x))

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::feature_store::N_FEATURES;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stump {
    pub feature_idx: usize,
    pub threshold: f64,
    pub left_value: f64,
    pub right_value: f64,
}

impl Stump {
    #[inline]
    pub fn predict(&self, x: &[f64]) -> f64 {
        if x[self.feature_idx] <= self.threshold { self.left_value } else { self.right_value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientBoostedModel {
    pub stumps: Vec<Stump>,
    pub learning_rate: f64,
    pub initial_log_odds: f64,
    pub n_games: usize,
}

impl GradientBoostedModel {
    pub fn new() -> Self {
        Self { stumps: Vec::new(), learning_rate: 0.1, initial_log_odds: 0.0, n_games: 0 }
    }

    /// Fit on (feature_vector, label) pairs. label = 1.0 for home win.
    pub fn fit(&mut self, data: &[([f64; N_FEATURES], f64)]) {
        if data.is_empty() { return; }
        let n = data.len();

        let pos = data.iter().filter(|&&(_, y)| y > 0.5).count() as f64;
        let base = (pos + 1.0) / (n as f64 + 2.0);
        self.initial_log_odds = (base / (1.0 - base)).ln();

        let mut f = vec![self.initial_log_odds; n];
        self.stumps.clear();

        let xs: Vec<[f64; N_FEATURES]> = data.iter().map(|(x, _)| *x).collect();
        let ys: Vec<f64> = data.iter().map(|(_, y)| *y).collect();

        for _ in 0..150 {
            let residuals: Vec<f64> = f.iter().zip(ys.iter()).map(|(fi, yi)| yi - sigmoid(*fi)).collect();
            let stump = fit_stump(&xs, &residuals);
            for (i, x) in xs.iter().enumerate() {
                f[i] += self.learning_rate * stump.predict(x);
            }
            self.stumps.push(stump);
        }

        self.n_games = n;
    }

    pub fn predict_proba(&self, x: &[f64; N_FEATURES]) -> f64 {
        let mut lo = self.initial_log_odds;
        for s in &self.stumps { lo += self.learning_rate * s.predict(x); }
        sigmoid(lo)
    }

    pub fn to_json(&self) -> Result<String> { Ok(serde_json::to_string(self)?) }
    pub fn from_json(s: &str) -> Result<Self> { Ok(serde_json::from_str(s)?) }
}

fn fit_stump(features: &[[f64; N_FEATURES]], residuals: &[f64]) -> Stump {
    let n = features.len();
    let total_sum: f64 = residuals.iter().sum();
    let total_sq: f64 = residuals.iter().map(|r| r * r).sum();
    let mean = total_sum / n as f64;

    let mut best = Stump { feature_idx: 0, threshold: 0.0, left_value: mean, right_value: mean };
    let mut best_loss = total_sq - total_sum * total_sum / n as f64;

    for fi in 0..N_FEATURES {
        let mut pairs: Vec<(f64, f64)> = features.iter().zip(residuals.iter())
            .map(|(x, r)| (x[fi], *r)).collect();
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut l_sum = 0.0f64;
        let mut l_sq = 0.0f64;
        let mut l_n = 0usize;

        for j in 0..n - 1 {
            let (v, r) = pairs[j];
            l_sum += r; l_sq += r * r; l_n += 1;
            // Only split at unique value boundaries
            if (pairs[j + 1].0 - v).abs() < 1e-10 { continue; }

            let r_n = n - l_n;
            let r_sum = total_sum - l_sum;
            let r_sq = total_sq - l_sq;

            let l_loss = l_sq - if l_n > 0 { l_sum * l_sum / l_n as f64 } else { 0.0 };
            let r_loss = r_sq - if r_n > 0 { r_sum * r_sum / r_n as f64 } else { 0.0 };
            let loss = l_loss + r_loss;

            if loss < best_loss {
                best_loss = loss;
                let thresh = (v + pairs[j + 1].0) / 2.0;
                let lv = if l_n > 0 { l_sum / l_n as f64 } else { 0.0 };
                let rv = if r_n > 0 { r_sum / r_n as f64 } else { 0.0 };
                best = Stump { feature_idx: fi, threshold: thresh, left_value: lv, right_value: rv };
            }
        }
    }
    best
}

#[inline]
pub fn sigmoid(x: f64) -> f64 { 1.0 / (1.0 + (-x).exp()) }
