//! Regularized Adjusted Plus-Minus (RAPM) — Team Level
//!
//! Design matrix X (n_games × n_teams):
//!   X[i, home_team] = +1, X[i, away_team] = -1
//! Target y[i] = home_pts − away_pts
//!
//! Ridge solution: β = (X'X + λI)⁻¹ X'y  (solved via Cholesky)
//!
//! Win probability: logistic(β_home − β_away + HCA) / scale

use anyhow::Result;
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RapmModel {
    pub ratings: HashMap<String, f64>, // team_id → adjusted plus-minus (pts/game)
    pub lambda: f64,
    pub n_games: usize,
}

impl RapmModel {
    pub fn new() -> Self {
        Self { ratings: HashMap::new(), lambda: 5.0, n_games: 0 }
    }

    /// Fit on games: (home_id, away_id, point_differential)
    pub fn fit(&mut self, games: &[(String, String, f64)]) {
        if games.len() < 5 { return; }

        let mut team_set = BTreeSet::new();
        for (h, a, _) in games {
            team_set.insert(h.clone());
            team_set.insert(a.clone());
        }
        let teams: Vec<String> = team_set.into_iter().collect();
        let team_idx: HashMap<&str, usize> = teams.iter().enumerate().map(|(i, t)| (t.as_str(), i)).collect();
        let nt = teams.len();
        let ng = games.len();

        let mut x_data = vec![0.0f64; ng * nt];
        let mut y_data = vec![0.0f64; ng];
        for (i, (h, a, margin)) in games.iter().enumerate() {
            if let Some(&hi) = team_idx.get(h.as_str()) { x_data[i * nt + hi] = 1.0; }
            if let Some(&ai) = team_idx.get(a.as_str()) { x_data[i * nt + ai] = -1.0; }
            y_data[i] = *margin;
        }

        let x = DMatrix::from_row_slice(ng, nt, &x_data);
        let y = DVector::from_vec(y_data);
        let xtx = x.transpose() * &x;
        let ridge = xtx + DMatrix::identity(nt, nt) * self.lambda;
        let xty = x.transpose() * y;

        if let Some(chol) = ridge.cholesky() {
            let beta = chol.solve(&xty);
            self.ratings.clear();
            for (i, tid) in teams.iter().enumerate() {
                self.ratings.insert(tid.clone(), beta[i]);
            }
        }

        self.n_games = ng;
    }

    pub fn predict_home_win_prob(&self, home_id: &str, away_id: &str) -> f64 {
        const HCA: f64 = 3.0;
        const SCALE: f64 = 11.0;
        let hr = self.ratings.get(home_id).copied().unwrap_or(0.0);
        let ar = self.ratings.get(away_id).copied().unwrap_or(0.0);
        let diff = hr - ar + HCA;
        1.0 / (1.0 + (-diff / SCALE).exp())
    }

    pub fn to_json(&self) -> Result<String> { Ok(serde_json::to_string(self)?) }
    pub fn from_json(s: &str) -> Result<Self> { Ok(serde_json::from_str(s)?) }
}
