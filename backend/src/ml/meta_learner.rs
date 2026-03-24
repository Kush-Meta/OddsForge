//! Stacked Meta-Learner + Isotonic Calibration
//!
//! Takes [p_poisson, p_rapm, p_gbt, p_mc, elo_diff_scaled,
//!        form_diff_scaled, net_rtg_diff_scaled, h2h_rate] → calibrated prob.
//!
//! Meta-learner: logistic regression fit via gradient descent on log-loss.
//! Calibration: Pool Adjacent Violators (isotonic regression).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::Row;

use crate::models::Match;
use super::{
    feature_store::{get_or_build_features, N_FEATURES},
    gradient_boosted::GradientBoostedModel,
    monte_carlo::{monte_carlo_win_prob, TeamFactors},
    poisson_model::PoissonModel,
    rapm::RapmModel,
};

pub const META_FEATURES: usize = 8;

/// Logistic regression meta-learner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaLearner {
    pub weights: [f64; META_FEATURES],
    pub bias: f64,
}

impl MetaLearner {
    pub fn new() -> Self {
        // Initialise with equal weight on all 4 base model outputs
        Self { weights: [0.5, 0.5, 0.5, 0.5, 0.01, 0.01, 0.02, 0.1], bias: 0.0 }
    }

    pub fn predict(&self, x: &[f64; META_FEATURES]) -> f64 {
        let logit = self.bias + self.weights.iter().zip(x.iter()).map(|(w, xi)| w * xi).sum::<f64>();
        1.0 / (1.0 + (-logit).exp())
    }

    pub fn fit(&mut self, data: &[([f64; META_FEATURES], f64)]) {
        if data.is_empty() { return; }
        let lr = 0.05;
        let l2 = 0.01;
        let n = data.len() as f64;
        for _ in 0..500 {
            let mut dw = [0.0f64; META_FEATURES];
            let mut db = 0.0f64;
            for (x, y) in data {
                let p = self.predict(x);
                let e = y - p;
                for i in 0..META_FEATURES { dw[i] += e * x[i]; }
                db += e;
            }
            for i in 0..META_FEATURES {
                self.weights[i] += lr * (dw[i] / n - l2 * self.weights[i]);
            }
            self.bias += lr * db / n;
        }
    }
}

/// Isotonic calibration via Pool Adjacent Violators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsotonicCalibrator {
    pub x_knots: Vec<f64>,
    pub y_knots: Vec<f64>,
}

impl IsotonicCalibrator {
    pub fn new() -> Self { Self { x_knots: vec![0.0, 1.0], y_knots: vec![0.0, 1.0] } }

    pub fn fit(&mut self, data: &[(f64, f64)]) {
        if data.len() < 4 { return; }
        let mut sorted = data.to_vec();
        sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // PAV: merge monotonicity-violating adjacent blocks
        // Each block: (mean_x, mean_y, weight)
        let mut blocks: Vec<(f64, f64, f64)> = sorted.iter().map(|(x, y)| (*x, *y, 1.0)).collect();
        let mut changed = true;
        while changed {
            changed = false;
            let mut i = 0;
            while i + 1 < blocks.len() {
                if blocks[i].1 > blocks[i + 1].1 {
                    let (x1, y1, w1) = blocks[i];
                    let (x2, y2, w2) = blocks[i + 1];
                    let w = w1 + w2;
                    let mx = (x1 * w1 + x2 * w2) / w;
                    let my = (y1 * w1 + y2 * w2) / w;
                    blocks[i] = (mx, my, w);
                    blocks.remove(i + 1);
                    changed = true;
                    if i > 0 { i -= 1; }
                } else {
                    i += 1;
                }
            }
        }
        self.x_knots = blocks.iter().map(|(x, _, _)| *x).collect();
        self.y_knots = blocks.iter().map(|(_, y, _)| *y).collect();
    }

    pub fn calibrate(&self, x: f64) -> f64 {
        if self.x_knots.is_empty() { return x; }
        if x <= self.x_knots[0] { return self.y_knots[0]; }
        if x >= *self.x_knots.last().unwrap() { return *self.y_knots.last().unwrap(); }
        let pos = self.x_knots.partition_point(|&k| k <= x);
        let i = pos.saturating_sub(1);
        if i + 1 >= self.x_knots.len() { return *self.y_knots.last().unwrap(); }
        let t = (x - self.x_knots[i]) / (self.x_knots[i + 1] - self.x_knots[i]);
        self.y_knots[i] + t * (self.y_knots[i + 1] - self.y_knots[i])
    }
}

/// Serializable complete ML model state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlModelState {
    pub poisson: PoissonModel,
    pub rapm: RapmModel,
    pub gbt: GradientBoostedModel,
    pub meta: MetaLearner,
    pub calibrator: IsotonicCalibrator,
    pub model_version: String,
}

impl MlModelState {
    pub fn new() -> Self {
        Self {
            poisson: PoissonModel::new(),
            rapm: RapmModel::new(),
            gbt: GradientBoostedModel::new(),
            meta: MetaLearner::new(),
            calibrator: IsotonicCalibrator::new(),
            model_version: "ml_v1.0_untrained".to_string(),
        }
    }
}

/// Build TeamFactors from a feature vector (pub(crate) for use in backtest)
pub(crate) fn team_factors_from_features(f: &[f64; N_FEATURES], home: bool) -> TeamFactors {
    if home {
        TeamFactors {
            efg_pct: f[7].clamp(0.3, 0.75),
            tov_pct: f[9].clamp(0.05, 0.30),
            oreb_pct: f[11].clamp(0.10, 0.55),
            ft_rate: f[13].clamp(0.05, 0.50),
            ft_pct: 0.77,
            pace: f[15].clamp(85.0, 120.0),
        }
    } else {
        TeamFactors {
            efg_pct: f[8].clamp(0.3, 0.75),
            tov_pct: f[10].clamp(0.05, 0.30),
            oreb_pct: f[12].clamp(0.10, 0.55),
            ft_rate: f[14].clamp(0.05, 0.50),
            ft_pct: 0.77,
            pace: f[16].clamp(85.0, 120.0),
        }
    }
}

/// Compute the 8-element meta feature vector from base model predictions + raw features
pub fn build_meta_features(
    p_poisson: f64, p_rapm: f64, p_gbt: f64, p_mc: f64,
    f: &[f64; N_FEATURES],
) -> [f64; META_FEATURES] {
    [
        p_poisson,
        p_rapm,
        p_gbt,
        p_mc,
        f[0] / 400.0,           // elo_diff scaled
        (f[24] - f[25]) / 20.0, // form diff scaled
        (f[1] - f[2]) / 10.0,   // net rating diff scaled
        f[19],                   // h2h rate
    ]
}

/// Top-level ML predictor — loads state and orchestrates all models
pub struct MlPredictor {
    pub state: Option<MlModelState>,
}

impl MlPredictor {
    pub fn new() -> Self { Self { state: None } }

    pub async fn load_from_db(&mut self, pool: &SqlitePool) -> Result<bool> {
        let row = sqlx::query(
            "SELECT params_json FROM model_params WHERE model_name = 'meta' ORDER BY version DESC LIMIT 1"
        ).fetch_optional(pool).await?;

        if let Some(r) = row {
            let json: String = r.get("params_json");
            match serde_json::from_str::<MlModelState>(&json) {
                Ok(state) => {
                    tracing::info!("Loaded ML model: {}", state.model_version);
                    self.state = Some(state);
                    return Ok(true);
                }
                Err(e) => tracing::warn!("Failed to deserialise ML model: {}", e),
            }
        }
        Ok(false)
    }

    pub fn set_state(&mut self, state: MlModelState) {
        self.state = Some(state);
    }

    /// Predict home win probability. Returns None → fall back to rule-based.
    pub async fn predict(&self, pool: &SqlitePool, m: &Match) -> Option<f64> {
        let state = self.state.as_ref()?;

        let features = get_or_build_features(pool, m).await.ok()?;
        let f = &features.0;

        let p_poisson = state.poisson.predict_home_win_prob(&m.home_team_id, &m.away_team_id);
        let p_rapm    = state.rapm.predict_home_win_prob(&m.home_team_id, &m.away_team_id);
        let p_gbt     = state.gbt.predict_proba(f);

        let hf = team_factors_from_features(f, true);
        let af = team_factors_from_features(f, false);
        let (p_mc, _) = monte_carlo_win_prob(&hf, &af, 1000);

        let mx = build_meta_features(p_poisson, p_rapm, p_gbt, p_mc, f);
        let raw = state.meta.predict(&mx);
        Some(state.calibrator.calibrate(raw).clamp(0.05, 0.95))
    }

    /// Get the Monte Carlo score distribution for a match
    pub async fn score_distribution(&self, pool: &SqlitePool, m: &Match) -> Option<Vec<f64>> {
        let state = self.state.as_ref()?;
        let features = get_or_build_features(pool, m).await.ok()?;
        let f = &features.0;
        let hf = team_factors_from_features(f, true);
        let af = team_factors_from_features(f, false);
        let (_, dist) = monte_carlo_win_prob(&hf, &af, 5000);
        Some(dist)
    }

    /// Permutation importance: for each feature, set it to 0 and measure probability delta.
    pub async fn feature_importance(&self, pool: &SqlitePool, m: &Match) -> Option<Vec<(String, f64, f64)>> {
        let state = self.state.as_ref()?;
        let features = get_or_build_features(pool, m).await.ok()?;
        let f = features.0;

        // Base prediction
        let p_base = self.predict(pool, m).await?;

        let feature_names = [
            "ELO Differential", "Home Net Rating", "Away Net Rating",
            "Home Off Rating", "Away Off Rating", "Home Def Rating", "Away Def Rating",
            "Home eFG%", "Away eFG%", "Home TOV%", "Away TOV%",
            "Home OREB%", "Away OREB%", "Home FT Rate", "Away FT Rate",
            "Home Pace", "Away Pace", "Home Win%", "Away Win%",
            "H2H Home Rate", "Home Rest Days", "Away Rest Days",
            "Home B2B", "Away B2B", "Home Form", "Away Form",
        ];

        let mut importances = Vec::new();
        for i in 0..N_FEATURES {
            let mut f_perm = f;
            f_perm[i] = 0.0; // zero out (mean-ish substitution)

            let p_po = state.poisson.predict_home_win_prob(&m.home_team_id, &m.away_team_id);
            let p_ra = state.rapm.predict_home_win_prob(&m.home_team_id, &m.away_team_id);
            let p_gb = state.gbt.predict_proba(&f_perm);

            let hf = team_factors_from_features(&f_perm, true);
            let af = team_factors_from_features(&f_perm, false);
            let (p_mc, _) = monte_carlo_win_prob(&hf, &af, 500);

            let mx = build_meta_features(p_po, p_ra, p_gb, p_mc, &f_perm);
            let p_perm = state.calibrator.calibrate(state.meta.predict(&mx)).clamp(0.05, 0.95);
            let contrib = p_base - p_perm;

            importances.push((
                feature_names.get(i).copied().unwrap_or("unknown").to_string(),
                f[i],
                contrib,
            ));
        }

        // Sort by absolute contribution
        importances.sort_by(|a, b| b.2.abs().partial_cmp(&a.2.abs()).unwrap_or(std::cmp::Ordering::Equal));
        Some(importances)
    }

    pub fn model_version(&self) -> String {
        self.state.as_ref().map(|s| s.model_version.clone()).unwrap_or_else(|| "rule_based_fallback".to_string())
    }

    pub fn has_model(&self) -> bool { self.state.is_some() }
}
