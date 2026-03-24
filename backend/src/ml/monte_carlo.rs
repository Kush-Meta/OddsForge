//! Monte Carlo Possession Simulation
//!
//! Simulates NBA games as Markov chains over possessions.
//! Per-possession outcomes are drawn from team Four Factors:
//!   eFG% (shooting), TOV% (turnovers), OREB% (offensive rebounds), FTr (free throws)
//!
//! Runs N simulations, returns P(home wins) + margin distribution.

use serde::{Deserialize, Serialize};

/// Minimal xorshift64 RNG — no external API surface to break across rand versions.
struct Xr64(u64);
impl Xr64 {
    fn new() -> Self {
        // Seed from system nanos for reasonable entropy
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ (d.as_secs() * 6364136223846793005))
            .unwrap_or(98765432109);
        Self(seed | 1) // ensure non-zero
    }
    #[inline]
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform [0, 1)
    #[inline]
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    #[inline]
    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

#[derive(Debug, Clone)]
pub struct TeamFactors {
    pub efg_pct: f64,
    pub tov_pct: f64,
    pub oreb_pct: f64,
    pub ft_rate: f64,
    pub ft_pct: f64,
    pub pace: f64, // possessions per 48 min
}

impl Default for TeamFactors {
    fn default() -> Self {
        Self { efg_pct: 0.52, tov_pct: 0.14, oreb_pct: 0.28, ft_rate: 0.22, ft_pct: 0.77, pace: 100.0 }
    }
}

/// Simulate one possession. Returns (points scored, possession switches).
fn sim_possession(off: &TeamFactors, def: &TeamFactors, rng: &mut Xr64) -> (f64, bool) {
    let adj_tov  = (off.tov_pct * 0.6 + (1.0 - def.tov_pct) * 0.4).clamp(0.06, 0.25);
    let adj_efg  = (off.efg_pct * 0.6 + (1.0 - def.efg_pct) * 0.4).clamp(0.35, 0.70);
    let adj_oreb = (off.oreb_pct * 0.55 + (1.0 - def.oreb_pct) * 0.45).clamp(0.12, 0.50);
    let adj_ftr  = (off.ft_rate * 0.6 + def.ft_rate * 0.4).clamp(0.06, 0.45);

    let r = rng.f64();
    let mut cum = 0.0;

    // Turnover
    cum += adj_tov;
    if r < cum { return (0.0, true); }

    // Free throws (simplified: ~25% of FTr leads to FT possession)
    cum += adj_ftr * 0.25;
    if r < cum {
        let pts = if rng.f64() < off.ft_pct { 1.0 } else { 0.0 }
                + if rng.f64() < off.ft_pct { 1.0 } else { 0.0 };
        return (pts, true);
    }

    // Shot (league avg ~36% 3-point rate)
    let is_three = rng.f64() < 0.36;
    if rng.f64() < adj_efg {
        (if is_three { 3.0 } else { 2.0 }, true)
    } else if rng.f64() < adj_oreb {
        (0.0, false) // offensive rebound
    } else {
        (0.0, true)  // defensive rebound
    }
}

fn sim_game(home: &TeamFactors, away: &TeamFactors, rng: &mut Xr64) -> (f64, f64) {
    let n_poss = ((home.pace + away.pace) / 2.0).round() as usize;
    let mut home_score = 1.5f64; // home court split: +3 pts shared as +1.5 / -1.5
    let mut away_score = 0.0f64;
    let mut home_ball = rng.bool();
    let mut count = 0;

    while count < n_poss {
        let (pts, switch) = if home_ball {
            sim_possession(home, away, rng)
        } else {
            sim_possession(away, home, rng)
        };
        if home_ball { home_score += pts; } else { away_score += pts; }
        if switch { home_ball = !home_ball; }
        count += 1;
    }
    (home_score.max(0.0), away_score.max(0.0))
}

/// Run Monte Carlo simulation.
/// Returns (p_home_win, margin_distribution[80]) where bucket i → margin = i−40.
pub fn monte_carlo_win_prob(home: &TeamFactors, away: &TeamFactors, n_sims: usize) -> (f64, Vec<f64>) {
    let mut rng = Xr64::new();
    let mut home_wins = 0usize;
    let mut buckets = vec![0usize; 80];

    for _ in 0..n_sims {
        let (hs, aws) = sim_game(home, away, &mut rng);
        if hs > aws { home_wins += 1; }
        let margin = (hs - aws).round() as i64;
        let idx = ((margin + 40).max(0).min(79)) as usize;
        buckets[idx] += 1;
    }

    let p = home_wins as f64 / n_sims as f64;
    let dist: Vec<f64> = buckets.iter().map(|&c| c as f64 / n_sims as f64).collect();
    (p, dist)
}
