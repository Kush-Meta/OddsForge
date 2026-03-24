//! Bayesian Poisson Scoring Model for NBA
//!
//! Adapts Dixon-Coles (1997) to basketball. Each team has:
//!   α_i (attack strength, log-parameterized)
//!   δ_j (defense strength, log-parameterized)
//!
//! Expected pace-adjusted score team i vs j (home):
//!   λ_i = exp(α_i + δ_j + μ + η)   where η is HCA
//!   λ_j = exp(α_j + δ_i + μ)
//!
//! Parameters estimated via MLE with gradient descent.
//! NBA innovation: uses raw points (pace-adjusted at training time).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for one team
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamParams {
    pub attack: f64,  // log attack strength
    pub defense: f64, // log defense strength (lower = better defense)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoissonModel {
    pub teams: HashMap<String, TeamParams>,
    pub mu: f64,   // log baseline scoring rate
    pub hca: f64,  // home court advantage (log scale)
    pub n_games: usize,
}

impl PoissonModel {
    pub fn new() -> Self {
        Self {
            teams: HashMap::new(),
            mu: 4.7, // ln(~110) — league avg pts/100pos
            hca: 0.025,
            n_games: 0,
        }
    }

    pub fn predict_home_win_prob(&self, home_id: &str, away_id: &str) -> f64 {
        let hp = match self.teams.get(home_id) { Some(p) => p, None => return 0.55 };
        let ap = match self.teams.get(away_id) { Some(p) => p, None => return 0.55 };

        let lambda_home = (hp.attack + ap.defense + self.mu + self.hca).exp();
        let lambda_away = (ap.attack + hp.defense + self.mu).exp();

        score_win_prob(lambda_home, lambda_away)
    }

    /// Fit via gradient descent on Poisson log-likelihood.
    /// games: (home_id, away_id, home_pts, away_pts)
    pub fn fit(&mut self, games: &[(String, String, f64, f64)]) {
        if games.is_empty() { return; }

        let mut all_teams = std::collections::HashSet::new();
        for (h, a, _, _) in games {
            all_teams.insert(h.clone());
            all_teams.insert(a.clone());
        }
        for t in &all_teams {
            self.teams.entry(t.clone()).or_insert(TeamParams { attack: 0.0, defense: 0.0 });
        }

        let lr = 0.01f64;
        let l2 = 0.02f64;
        let n = games.len() as f64;

        for _ in 0..300 {
            let mut grad_atk: HashMap<String, f64> = HashMap::new();
            let mut grad_def: HashMap<String, f64> = HashMap::new();
            let mut g_mu = 0.0f64;
            let mut g_hca = 0.0f64;

            for (hi, ai, ph, pa) in games {
                let hp = self.teams.get(hi).cloned().unwrap_or(TeamParams { attack: 0.0, defense: 0.0 });
                let ap = self.teams.get(ai).cloned().unwrap_or(TeamParams { attack: 0.0, defense: 0.0 });

                let lh = (hp.attack + ap.defense + self.mu + self.hca).exp();
                let la = (ap.attack + hp.defense + self.mu).exp();

                let dlh = ph - lh;
                let dla = pa - la;

                *grad_atk.entry(hi.clone()).or_insert(0.0) += dlh;
                *grad_def.entry(ai.clone()).or_insert(0.0) += dlh;
                *grad_atk.entry(ai.clone()).or_insert(0.0) += dla;
                *grad_def.entry(hi.clone()).or_insert(0.0) += dla;
                g_mu += dlh + dla;
                g_hca += dlh;
            }

            for (tid, p) in self.teams.iter_mut() {
                p.attack += lr * (grad_atk.get(tid).copied().unwrap_or(0.0) / n - l2 * p.attack);
                p.defense += lr * (grad_def.get(tid).copied().unwrap_or(0.0) / n - l2 * p.defense);
            }
            self.mu += lr * g_mu / n;
            self.hca += lr * g_hca / n;

            // Identification: zero-mean attacks
            if !self.teams.is_empty() {
                let mean: f64 = self.teams.values().map(|p| p.attack).sum::<f64>() / self.teams.len() as f64;
                for p in self.teams.values_mut() { p.attack -= mean; }
            }
        }

        self.n_games = games.len();
    }

    pub fn to_json(&self) -> Result<String> { Ok(serde_json::to_string(self)?) }
    pub fn from_json(s: &str) -> Result<Self> { Ok(serde_json::from_str(s)?) }
}

/// P(home_score > away_score) via Poisson PMF convolution
fn score_win_prob(lambda_home: f64, lambda_away: f64) -> f64 {
    let lh = lambda_home.max(70.0).min(160.0);
    let la = lambda_away.max(70.0).min(160.0);
    let max_s = 180usize;
    let home_pmf = poisson_pmf(lh, max_s);
    let away_pmf = poisson_pmf(la, max_s);

    let mut p_win = 0.0f64;
    let mut p_tie = 0.0f64;
    for h in 0..max_s {
        for a in 0..max_s {
            let p = home_pmf[h] * away_pmf[a];
            if h > a { p_win += p; }
            else if h == a { p_tie += p; }
        }
    }
    // Basketball: OT resolves ties; split tie prob by scoring rate
    p_win + p_tie * lh / (lh + la)
}

fn poisson_pmf(lambda: f64, max_k: usize) -> Vec<f64> {
    let mut pmf = vec![0.0f64; max_k];
    let log_lam = lambda.ln();
    let mut log_fact = 0.0f64;
    for k in 0..max_k {
        if k > 0 { log_fact += (k as f64).ln(); }
        pmf[k] = (k as f64 * log_lam - lambda - log_fact).exp();
    }
    pmf
}
