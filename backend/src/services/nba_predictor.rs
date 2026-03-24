//! NBA Game Prediction Engine — Revolutionary 5-Component Ensemble
//!
//! ## Architecture
//!
//! When NBA Stats API data is available (stored in `nba_advanced_stats`):
//!
//! | Component               | Early season | Late season | Description                          |
//! |-------------------------|-------------|-------------|--------------------------------------|
//! | Bayesian Net Rating     | 15%         | 35%         | ORtg-DRtg, shrunk toward prior       |
//! | MOV-Adjusted ELO        | 30%         | 15%         | K=20, +75 HCA, log(MOV) multiplier   |
//! | Opponent-Adjusted Form  | 25%         | 25%         | Rolling point diff, SOS-adjusted     |
//! | Dean Oliver Four Factors| 5%          | 20%         | eFG%, TOV%, OREB%, FTr (both sides)  |
//! | Head-to-Head            | 25%         | 5%          | Historical record, heavily regressed |
//!
//! Weights shift dynamically based on games played (Bayesian credibility).
//!
//! Fallback (no NBA Stats data): ELO 40% + Form 40% + H2H 20%
//!
//! ## Post-ensemble schedule adjustments
//! - Back-to-back (0 rest days):        ±5.0 pp
//! - 3-in-4 nights (1 rest + prior 2):  ±2.5 pp
//! - Road trip fatigue (3+ away):       ±1.5 pp (benefits home team)

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use std::sync::OnceLock;
use uuid::Uuid;

use crate::db::get_nba_advanced_stats;
use crate::ml::meta_learner::{MlModelState, MlPredictor};
use crate::models::{Match, NbaAdvancedStats, Prediction};

// ── Global ML state (tokio RwLock so guards are Send across awaits) ───────────

static ML_LOCK: OnceLock<tokio::sync::RwLock<MlPredictor>> = OnceLock::new();

fn ml_lock() -> &'static tokio::sync::RwLock<MlPredictor> {
    ML_LOCK.get_or_init(|| tokio::sync::RwLock::new(MlPredictor::new()))
}

/// Load the latest ML model from DB into the global predictor.
pub async fn load_ml_model(pool: &SqlitePool) -> Result<bool> {
    let mut tmp = MlPredictor::new();
    let loaded = tmp.load_from_db(pool).await?;
    if loaded {
        let mut guard = ml_lock().write().await;
        *guard = tmp;
    }
    Ok(loaded)
}

/// Replace the global ML model with a freshly trained state (async).
pub async fn set_ml_model(state: MlModelState) {
    let mut guard = ml_lock().write().await;
    guard.set_state(state);
}

// ── Constants ────────────────────────────────────────────────────────────────

/// Home court advantage expressed as equivalent net rating points.
/// Calibrated so equal teams produce ~59% home win probability.
const NBA_HCA_NET_RATING: f64 = 3.2;

/// Home court advantage in raw points per game (used by form model).
const NBA_HCA_POINTS: f64 = 3.0;

/// Number of "prior games" for Bayesian shrinkage.
/// At exactly this many games played, the rating receives full credibility.
const BAYESIAN_PRIOR_GAMES: f64 = 55.0;

/// ELO home court advantage in ELO points.
/// +75 → equal teams: 1/(1+10^(-75/400)) ≈ 60.7% — slightly above NBA empirical ~59%.
const NBA_ELO_HCA: f64 = 75.0;

// ── Predictor ────────────────────────────────────────────────────────────────

pub struct NbaPredictor;

impl NbaPredictor {
    pub fn new() -> Self {
        Self
    }

    /// Generate a prediction for a single NBA game.
    pub async fn predict(&self, pool: &SqlitePool, match_data: &Match) -> Result<Prediction> {
        // ── Load team ELO ratings ────────────────────────────────────────────
        let home_elo = self.get_elo(pool, &match_data.home_team_id).await?;
        let away_elo = self.get_elo(pool, &match_data.away_team_id).await?;

        // ── Load advanced stats (absent until NBA Stats API is fetched) ──────
        let home_adv = get_nba_advanced_stats(pool, &match_data.home_team_id)
            .await
            .ok()
            .flatten();
        let away_adv = get_nba_advanced_stats(pool, &match_data.away_team_id)
            .await
            .ok()
            .flatten();
        let has_advanced = home_adv.is_some() && away_adv.is_some();

        // ── Run all 5 models ─────────────────────────────────────────────────
        let net_rating_prob = match (&home_adv, &away_adv) {
            (Some(h), Some(a)) => self.net_rating_model(h, a),
            _ => 0.55, // NBA baseline home win rate
        };

        let elo_prob = self.elo_model(home_elo, away_elo);

        let form_prob = self
            .form_model(pool, &match_data.home_team_id, &match_data.away_team_id)
            .await?;

        let ff_prob = match (&home_adv, &away_adv) {
            (Some(h), Some(a)) => self.four_factors_model(h, a),
            _ => 0.55,
        };

        let h2h_prob = self
            .h2h_model(pool, &match_data.home_team_id, &match_data.away_team_id)
            .await?;

        // ── Dynamic ensemble weights ─────────────────────────────────────────
        let home_games = home_adv.as_ref().map(|s| s.games_played).unwrap_or(0);
        let away_games = away_adv.as_ref().map(|s| s.games_played).unwrap_or(0);

        let raw_home_prob = if has_advanced {
            let (w_nr, w_elo, w_form, w_ff, w_h2h) =
                Self::ensemble_weights(home_games, away_games);
            net_rating_prob * w_nr
                + elo_prob * w_elo
                + form_prob * w_form
                + ff_prob * w_ff
                + h2h_prob * w_h2h
        } else {
            // Fallback: no advanced stats available
            elo_prob * 0.40 + form_prob * 0.40 + h2h_prob * 0.20
        };

        // ── Schedule adjustment (post-ensemble) ──────────────────────────────
        let schedule_delta = self
            .schedule_adjustment(pool, match_data)
            .await
            .unwrap_or(0.0);

        let final_home = (raw_home_prob + schedule_delta).clamp(0.05, 0.95);
        let final_away = 1.0 - final_home;

        // ── Confidence score ─────────────────────────────────────────────────
        let model_probs = [net_rating_prob, elo_prob, form_prob, ff_prob, h2h_prob];
        let confidence = self.compute_confidence(&model_probs, final_home, has_advanced);

        // ── Try ML predictor first ────────────────────────────────────────────
        {
            let guard = ml_lock().read().await;
            if let Some(ml_prob) = guard.predict(pool, match_data).await {
                let ml_away = 1.0 - ml_prob;
                // Confidence: blend inter-model agreement with ML output strength
                let ml_conf = 0.5 + (ml_prob - 0.5).abs();
                return Ok(Prediction {
                    id: Uuid::new_v4().to_string(),
                    match_id: match_data.id.clone(),
                    home_win_probability: ml_prob,
                    away_win_probability: ml_away,
                    draw_probability: None,
                    model_version: guard.model_version(),
                    confidence_score: ml_conf,
                    created_at: Utc::now(),
                });
            }
        }

        // ── Fallback: rule-based ensemble ─────────────────────────────────────
        let model_version = if has_advanced {
            "nba_v3.0"
        } else {
            "nba_v3.0_fallback"
        };

        Ok(Prediction {
            id: Uuid::new_v4().to_string(),
            match_id: match_data.id.clone(),
            home_win_probability: final_home,
            away_win_probability: final_away,
            draw_probability: None,
            model_version: model_version.to_string(),
            confidence_score: confidence,
            created_at: Utc::now(),
        })
    }

    // ── Model 1: Bayesian Net Rating ─────────────────────────────────────────

    /// Converts Bayesian-shrunk net ratings to a home win probability.
    ///
    /// Shrinkage means early-season outlier ratings get pulled toward 0 (league avg).
    /// The 0.12 logistic coefficient calibrates to: +1 NetRtg ≈ +3% win probability.
    fn net_rating_model(&self, home: &NbaAdvancedStats, away: &NbaAdvancedStats) -> f64 {
        let shrunk_home = bayesian_shrinkage(home.net_rating, home.games_played);
        let shrunk_away = bayesian_shrinkage(away.net_rating, away.games_played);
        sigmoid((shrunk_home - shrunk_away + NBA_HCA_NET_RATING) * 0.12)
    }

    // ── Model 2: ELO ─────────────────────────────────────────────────────────

    /// NBA-calibrated ELO: K=20, +75 HCA, no draw outcome.
    pub fn elo_model(&self, home_elo: f64, away_elo: f64) -> f64 {
        1.0 / (1.0 + 10f64.powf((away_elo - (home_elo + NBA_ELO_HCA)) / 400.0))
    }

    /// Logarithmic margin-of-victory multiplier for ELO updates.
    ///
    /// ln(1 + margin) grows quickly for small margins and flattens for blowouts,
    /// preventing large margins from distorting ratings.
    pub fn mov_multiplier(margin: i32) -> f64 {
        let m = margin.unsigned_abs() as f64;
        (1.0 + m.ln_1p() * 0.45).min(2.5)
    }

    // ── Model 3: Opponent-Adjusted Rolling Form ───────────────────────────────

    /// Exponentially-weighted point differential over the last 15 contextual games,
    /// adjusted for opponent strength via their Bayesian net rating.
    ///
    /// adjusted_margin = actual_margin − (opponent_net_rating / 3)
    ///
    /// This means beating a +9 NetRtg team by 8 is equivalent to beating a 0 NetRtg
    /// team by 11, correctly rewarding wins over stronger opponents.
    async fn form_model(
        &self,
        pool: &SqlitePool,
        home_id: &str,
        away_id: &str,
    ) -> Result<f64> {
        let home_score = self.rolling_adj_form(pool, home_id, true).await?;
        let away_score = self.rolling_adj_form(pool, away_id, false).await?;
        // form_diff ∈ approx [-15, +15]; adding HCA before sigmoid
        Ok(sigmoid((home_score - away_score + NBA_HCA_POINTS) * 0.10))
    }

    async fn rolling_adj_form(
        &self,
        pool: &SqlitePool,
        team_id: &str,
        home_context: bool,
    ) -> Result<f64> {
        struct GameRow {
            home_team_id: String,
            away_team_id: String,
            home_score: i32,
            away_score: i32,
        }

        let sql_home = "SELECT home_team_id, away_team_id, home_score, away_score
                        FROM matches
                        WHERE home_team_id = ? AND status = 'finished'
                          AND home_score IS NOT NULL AND sport = 'basketball'
                        ORDER BY match_date DESC LIMIT 15";

        let sql_away = "SELECT home_team_id, away_team_id, home_score, away_score
                        FROM matches
                        WHERE away_team_id = ? AND status = 'finished'
                          AND away_score IS NOT NULL AND sport = 'basketball'
                        ORDER BY match_date DESC LIMIT 15";

        let sql = if home_context { sql_home } else { sql_away };

        let games: Vec<GameRow> = sqlx::query(sql)
            .bind(team_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .filter_map(|r| {
                Some(GameRow {
                    home_team_id: r.try_get("home_team_id").ok()?,
                    away_team_id: r.try_get("away_team_id").ok()?,
                    home_score: r.try_get("home_score").ok()?,
                    away_score: r.try_get("away_score").ok()?,
                })
            })
            .collect();

        if games.is_empty() {
            return Ok(0.0);
        }

        let mut weighted_margin = 0.0_f64;
        let mut weight_total = 0.0_f64;
        let decay = 0.90_f64;

        for (i, game) in games.iter().enumerate() {
            let w = decay.powi(i as i32);
            let is_home = game.home_team_id == team_id;
            let (ts, os, opp_id) = if is_home {
                (game.home_score, game.away_score, &game.away_team_id)
            } else {
                (game.away_score, game.home_score, &game.home_team_id)
            };

            let raw_margin = (ts - os) as f64;

            // Opponent-strength adjustment
            let opp_net = get_nba_advanced_stats(pool, opp_id)
                .await
                .ok()
                .flatten()
                .map(|s| bayesian_shrinkage(s.net_rating, s.games_played))
                .unwrap_or(0.0);

            // Subtract opponent's expected contribution:
            // If they're a +9 team, they'd be expected to contribute ~+3 pts to your margin
            let adj_margin = raw_margin - (opp_net / 3.0);
            weighted_margin += w * adj_margin;
            weight_total += w;
        }

        Ok(if weight_total > 0.0 {
            weighted_margin / weight_total
        } else {
            0.0
        })
    }

    // ── Model 4: Dean Oliver Four Factors ────────────────────────────────────

    /// Implements Dean Oliver's Four Factors model from "Basketball on Paper" (2004).
    ///
    /// Both offensive and defensive factors are computed, giving a complete team score.
    /// Weights: eFG% 40% · TOV% 25% (inverted) · OREB% 20% · FTr 15%
    fn four_factors_model(&self, home: &NbaAdvancedStats, away: &NbaAdvancedStats) -> f64 {
        let home_off = four_factors_score(home.efg_pct, home.tov_pct, home.oreb_pct, home.ft_rate);
        let home_def = four_factors_score(
            1.0 - home.opp_efg_pct,
            home.opp_tov_pct,        // force opponent turnovers = better defense
            1.0 - home.opp_oreb_pct, // deny opponent offensive boards
            1.0 - home.opp_ft_rate,  // prevent opponent free throws
        );

        let away_off = four_factors_score(away.efg_pct, away.tov_pct, away.oreb_pct, away.ft_rate);
        let away_def = four_factors_score(
            1.0 - away.opp_efg_pct,
            away.opp_tov_pct,
            1.0 - away.opp_oreb_pct,
            1.0 - away.opp_ft_rate,
        );

        let home_total = (home_off + home_def) / 2.0;
        let away_total = (away_off + away_def) / 2.0;
        let ff_diff = home_total - away_total;

        // 5.0 scale: ±0.06 typical advantage → ±23% probability swing
        // 0.30 logit for NBA home court (~57% for equal teams from this component)
        sigmoid(ff_diff * 5.0 + 0.30)
    }

    // ── Model 5: Head-to-Head ────────────────────────────────────────────────

    /// Historical H2H record, Bayesian-regressed toward the NBA home baseline (55%).
    /// Full credibility is capped at 70% even with many H2H games, because matchup
    /// history can be misleading when rosters change.
    async fn h2h_model(
        &self,
        pool: &SqlitePool,
        home_id: &str,
        away_id: &str,
    ) -> Result<f64> {
        let rows = sqlx::query(
            "SELECT home_team_id, home_score, away_score FROM matches
             WHERE ((home_team_id = ? AND away_team_id = ?)
                 OR (home_team_id = ? AND away_team_id = ?))
               AND status = 'finished' AND sport = 'basketball'
             ORDER BY match_date DESC LIMIT 10",
        )
        .bind(home_id)
        .bind(away_id)
        .bind(away_id)
        .bind(home_id)
        .fetch_all(pool)
        .await?;

        let mut home_wins = 0u32;
        let mut total = 0u32;

        for row in &rows {
            let row_home_id: String = match row.try_get("home_team_id") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let hs: i32 = match row.try_get("home_score") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let aws: i32 = match row.try_get("away_score") {
                Ok(v) => v,
                Err(_) => continue,
            };
            total += 1;
            let current_home_won = hs > aws;
            // Did the team currently playing at home win this historical game?
            if (row_home_id == home_id && current_home_won)
                || (row_home_id == away_id && !current_home_won)
            {
                home_wins += 1;
            }
        }

        if total == 0 {
            return Ok(0.55); // NBA home baseline
        }

        let raw = home_wins as f64 / total as f64;
        // Credibility grows with sample but caps at 70%
        let credibility = (total as f64 / 20.0).min(0.70);
        Ok(raw * credibility + 0.55 * (1.0 - credibility))
    }

    // ── Schedule Adjustment ──────────────────────────────────────────────────

    /// Returns Δ added to the home win probability (negative = hurts home team).
    async fn schedule_adjustment(
        &self,
        pool: &SqlitePool,
        match_data: &Match,
    ) -> Result<f64> {
        let home_rest =
            self.rest_days(pool, &match_data.home_team_id, match_data.match_date).await?;
        let away_rest =
            self.rest_days(pool, &match_data.away_team_id, match_data.match_date).await?;

        let mut delta = 0.0_f64;

        // Back-to-back penalties (most impactful schedule factor in NBA)
        if home_rest == Some(0) {
            delta -= 0.05;
        }
        if away_rest == Some(0) {
            delta += 0.05; // Away team fatigued → better for home team
        }

        // 3-in-4 nights (moderate fatigue: 1 rest day but also played 2 nights ago)
        if home_rest == Some(1)
            && self
                .played_two_nights_ago(pool, &match_data.home_team_id, match_data.match_date)
                .await?
        {
            delta -= 0.025;
        }
        if away_rest == Some(1)
            && self
                .played_two_nights_ago(pool, &match_data.away_team_id, match_data.match_date)
                .await?
        {
            delta += 0.025;
        }

        // Road trip fatigue: away team on 3+ consecutive away games
        let away_consecutive = self
            .consecutive_away_games(pool, &match_data.away_team_id, match_data.match_date)
            .await?;
        if away_consecutive >= 3 {
            delta += 0.015;
        }

        Ok(delta)
    }

    /// Days of rest before this game (0 = back-to-back, None = no prior game on record).
    async fn rest_days(
        &self,
        pool: &SqlitePool,
        team_id: &str,
        game_date: DateTime<Utc>,
    ) -> Result<Option<u32>> {
        let last = sqlx::query(
            "SELECT match_date FROM matches
             WHERE (home_team_id = ? OR away_team_id = ?)
               AND status = 'finished'
               AND match_date < ?
             ORDER BY match_date DESC LIMIT 1",
        )
        .bind(team_id)
        .bind(team_id)
        .bind(game_date.to_rfc3339())
        .fetch_optional(pool)
        .await?;

        Ok(last.and_then(|row| {
            let date_str: String = row.try_get("match_date").ok()?;
            let last_date = chrono::DateTime::parse_from_rfc3339(&date_str)
                .map(|d| d.with_timezone(&Utc))
                .ok()?;
            let days = (game_date - last_date).num_days().max(0) as u32;
            Some(days.saturating_sub(1).min(7))
        }))
    }

    /// True if the team played a game 2 days before `game_date`.
    async fn played_two_nights_ago(
        &self,
        pool: &SqlitePool,
        team_id: &str,
        game_date: DateTime<Utc>,
    ) -> Result<bool> {
        let lo = (game_date - chrono::Duration::days(3)).to_rfc3339();
        let hi = (game_date - chrono::Duration::days(2)).to_rfc3339();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM matches
             WHERE (home_team_id = ? OR away_team_id = ?)
               AND status = 'finished'
               AND match_date >= ? AND match_date <= ?",
        )
        .bind(team_id)
        .bind(team_id)
        .bind(&lo)
        .bind(&hi)
        .fetch_one(pool)
        .await?;
        Ok(count > 0)
    }

    /// Number of consecutive away games the team has played before `before`.
    async fn consecutive_away_games(
        &self,
        pool: &SqlitePool,
        team_id: &str,
        before: DateTime<Utc>,
    ) -> Result<u32> {
        let rows = sqlx::query(
            "SELECT home_team_id FROM matches
             WHERE (home_team_id = ? OR away_team_id = ?)
               AND status = 'finished'
               AND match_date < ?
             ORDER BY match_date DESC LIMIT 6",
        )
        .bind(team_id)
        .bind(team_id)
        .bind(before.to_rfc3339())
        .fetch_all(pool)
        .await?;

        let mut streak = 0u32;
        for row in &rows {
            let home_id: String = match row.try_get("home_team_id") {
                Ok(v) => v,
                Err(_) => break,
            };
            if home_id == team_id {
                break; // Home game ends the road trip
            }
            streak += 1;
        }
        Ok(streak)
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    async fn get_elo(&self, pool: &SqlitePool, team_id: &str) -> Result<f64> {
        let elo: Option<f64> =
            sqlx::query_scalar("SELECT elo_rating FROM teams WHERE id = ?")
                .bind(team_id)
                .fetch_optional(pool)
                .await?;
        Ok(elo.unwrap_or(1200.0))
    }

    /// Dynamic weights that shift as the season progresses.
    ///
    /// Early season: ELO and H2H fill the role of net rating and four factors
    /// because there's too little data for those to be reliable.
    ///
    /// Late season: net rating and four factors take over as the dominant signals.
    fn ensemble_weights(home_games: i32, away_games: i32) -> (f64, f64, f64, f64, f64) {
        let g = home_games.min(away_games) as f64;
        let d = (g / BAYESIAN_PRIOR_GAMES).min(1.0); // 0.0 = start of season, 1.0 = full season

        let w_nr   = 0.15 + 0.20 * d; // Net Rating:   15% → 35%
        let w_elo  = 0.30 - 0.15 * d; // ELO:          30% → 15%
        let w_form = 0.25;             // Form:         25% (constant — always informative)
        let w_ff   = 0.05 + 0.15 * d; // Four Factors:  5% → 20%
        let w_h2h  = 0.25 - 0.20 * d; // H2H:          25% →  5%

        let total = w_nr + w_elo + w_form + w_ff + w_h2h;
        (
            w_nr / total,
            w_elo / total,
            w_form / total,
            w_ff / total,
            w_h2h / total,
        )
    }

    /// Confidence = blend of prediction strength + inter-model agreement + data quality bonus.
    fn compute_confidence(
        &self,
        model_probs: &[f64],
        final_home_prob: f64,
        has_advanced: bool,
    ) -> f64 {
        let best = final_home_prob.max(1.0 - final_home_prob);
        let strength = ((best - 0.5) * 2.5).clamp(0.0, 1.0);

        let n = model_probs.len() as f64;
        let mean = model_probs.iter().sum::<f64>() / n;
        let std_dev = (model_probs.iter().map(|&p| (p - mean).powi(2)).sum::<f64>() / n).sqrt();
        let agreement = (1.0 - std_dev / 0.15).clamp(0.0, 1.0);

        let data_bonus = if has_advanced { 0.05 } else { 0.0 };

        (0.40 + 0.30 * strength + 0.20 * agreement + data_bonus).clamp(0.40, 0.95)
    }
}

// ── Pure Mathematical Functions ──────────────────────────────────────────────

/// Logistic sigmoid: σ(x) = 1 / (1 + e^{−x})
#[inline]
pub fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Bayesian shrinkage of an observed statistic toward a league-average prior of 0.
///
/// With `games_played < BAYESIAN_PRIOR_GAMES` the rating is pulled toward zero.
/// At exactly `BAYESIAN_PRIOR_GAMES` games it is returned unchanged.
#[inline]
pub fn bayesian_shrinkage(observed: f64, games_played: i32) -> f64 {
    let credibility = (games_played as f64 / BAYESIAN_PRIOR_GAMES).min(1.0);
    observed * credibility
}

/// Dean Oliver's Four Factors offensive score.
///
/// `tov` should be the *raw* turnover rate for the offensive side (lower = better).
/// Pass `1.0 - opp_stat` when scoring the defensive side.
#[inline]
pub fn four_factors_score(efg: f64, tov: f64, oreb: f64, ftr: f64) -> f64 {
    0.40 * efg + 0.25 * (1.0 - tov) + 0.20 * oreb + 0.15 * ftr
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── sigmoid ──────────────────────────────────────────────────────────────

    #[test]
    fn sigmoid_midpoint_is_half() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn sigmoid_symmetry() {
        for &x in &[0.5, 1.0, 2.0, 5.0] {
            assert!((sigmoid(x) + sigmoid(-x) - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn sigmoid_extreme_values() {
        assert!(sigmoid(20.0) > 0.999);
        assert!(sigmoid(-20.0) < 0.001);
    }

    // ── bayesian_shrinkage ───────────────────────────────────────────────────

    #[test]
    fn shrinkage_at_zero_games_returns_zero() {
        assert_eq!(bayesian_shrinkage(10.0, 0), 0.0);
    }

    #[test]
    fn shrinkage_at_full_games_returns_observed() {
        let full = BAYESIAN_PRIOR_GAMES as i32;
        assert!((bayesian_shrinkage(8.5, full) - 8.5).abs() < 0.01);
    }

    #[test]
    fn shrinkage_midpoint() {
        let half = (BAYESIAN_PRIOR_GAMES / 2.0) as i32;
        let shrunk = bayesian_shrinkage(10.0, half);
        assert!(shrunk > 4.0 && shrunk < 6.0, "half-season shrinkage should be ~50% of observed");
    }

    #[test]
    fn shrinkage_caps_at_one() {
        // More games than prior → capped at full credibility
        assert!((bayesian_shrinkage(6.0, 200) - 6.0).abs() < 0.01);
    }

    // ── four_factors_score ───────────────────────────────────────────────────

    #[test]
    fn four_factors_average_team() {
        // Typical NBA averages: eFG=0.52, TOV=0.14, OREB=0.28, FTr=0.24
        let score = four_factors_score(0.52, 0.14, 0.28, 0.24);
        // Expected: 0.40*0.52 + 0.25*0.86 + 0.20*0.28 + 0.15*0.24
        //         = 0.208 + 0.215 + 0.056 + 0.036 = 0.515
        assert!((score - 0.515).abs() < 0.001, "avg team score should be ~0.515, got {}", score);
    }

    #[test]
    fn four_factors_better_shooting_raises_score() {
        let base = four_factors_score(0.52, 0.14, 0.28, 0.24);
        let better = four_factors_score(0.57, 0.14, 0.28, 0.24); // +5% eFG
        assert!(better > base, "better eFG should raise score");
    }

    #[test]
    fn four_factors_lower_tov_raises_score() {
        let base = four_factors_score(0.52, 0.14, 0.28, 0.24);
        let fewer_tov = four_factors_score(0.52, 0.10, 0.28, 0.24); // -4% TOV
        assert!(fewer_tov > base, "fewer turnovers should raise score");
    }

    // ── elo_model ────────────────────────────────────────────────────────────

    #[test]
    fn elo_equal_teams_home_advantage() {
        let p = NbaPredictor::new();
        let prob = p.elo_model(1200.0, 1200.0);
        // +75 HCA → ~60.7% home win rate for equal teams
        assert!(prob > 0.60 && prob < 0.62, "equal teams should give ~60.7%, got {:.3}", prob);
    }

    #[test]
    fn elo_strong_home_team() {
        let p = NbaPredictor::new();
        let prob = p.elo_model(1400.0, 1200.0);
        assert!(prob > 0.75, "strong home team should have >75% win prob");
    }

    #[test]
    fn elo_strong_away_team() {
        let p = NbaPredictor::new();
        let prob = p.elo_model(1200.0, 1400.0);
        assert!(prob < 0.40, "strong away team should give <40% to home side");
    }

    // ── net_rating_model ─────────────────────────────────────────────────────

    #[test]
    fn net_rating_equal_teams_home_advantage() {
        let p = NbaPredictor::new();
        let home = NbaAdvancedStats {
            team_id: "h".into(), off_rating: 113.0, def_rating: 113.0, net_rating: 0.0,
            pace: 100.0, efg_pct: 0.52, opp_efg_pct: 0.52, tov_pct: 0.14,
            opp_tov_pct: 0.14, oreb_pct: 0.28, opp_oreb_pct: 0.28, ft_rate: 0.24,
            opp_ft_rate: 0.24, games_played: 55, wins: 27, season: "2025-26".into(),
            fetched_at: "2026-01-01T00:00:00Z".into(),
        };
        let away = home.clone();
        let prob = p.net_rating_model(&home, &away);
        // Equal teams, full credibility: HCA should push home to ~59%
        assert!(prob > 0.57 && prob < 0.62, "equal teams net-rating model should give ~59%, got {:.3}", prob);
    }

    #[test]
    fn net_rating_better_home_team() {
        let p = NbaPredictor::new();
        let make_stats = |net: f64, gp: i32| NbaAdvancedStats {
            team_id: "x".into(), off_rating: 113.0 + net, def_rating: 113.0,
            net_rating: net, pace: 100.0, efg_pct: 0.52, opp_efg_pct: 0.52,
            tov_pct: 0.14, opp_tov_pct: 0.14, oreb_pct: 0.28, opp_oreb_pct: 0.28,
            ft_rate: 0.24, opp_ft_rate: 0.24, games_played: gp, wins: 30,
            season: "2025-26".into(), fetched_at: "2026-01-01T00:00:00Z".into(),
        };
        let home = make_stats(8.0, 55);  // +8 NetRtg (elite team)
        let away = make_stats(-3.0, 55); // -3 NetRtg (below average)
        let prob = p.net_rating_model(&home, &away);
        // +11 net diff + 3.2 HCA = 14.2; sigmoid(14.2 * 0.12) = sigmoid(1.704) ≈ 0.846
        assert!(prob > 0.80, "elite home vs poor away should give >80%, got {:.3}", prob);
    }

    #[test]
    fn net_rating_early_season_shrinkage() {
        let p = NbaPredictor::new();
        let make_stats = |net: f64, gp: i32| NbaAdvancedStats {
            team_id: "x".into(), off_rating: 113.0, def_rating: 113.0 - net,
            net_rating: net, pace: 100.0, efg_pct: 0.52, opp_efg_pct: 0.52,
            tov_pct: 0.14, opp_tov_pct: 0.14, oreb_pct: 0.28, opp_oreb_pct: 0.28,
            ft_rate: 0.24, opp_ft_rate: 0.24, games_played: gp, wins: 5,
            season: "2025-26".into(), fetched_at: "2026-01-01T00:00:00Z".into(),
        };
        let late_home = make_stats(10.0, 55);
        let late_away = make_stats(-5.0, 55);
        let early_home = make_stats(10.0, 5);
        let early_away = make_stats(-5.0, 5);

        let prob_late  = p.net_rating_model(&late_home, &late_away);
        let prob_early = p.net_rating_model(&early_home, &early_away);

        assert!(
            prob_late > prob_early,
            "late-season prediction should be more extreme than early-season ({:.3} vs {:.3})",
            prob_late, prob_early
        );
    }

    // ── mov_multiplier ───────────────────────────────────────────────────────

    #[test]
    fn mov_multiplier_one_point_game_is_near_one() {
        let mult = NbaPredictor::mov_multiplier(1);
        assert!(mult > 1.0 && mult < 1.4);
    }

    #[test]
    fn mov_multiplier_grows_with_margin() {
        assert!(NbaPredictor::mov_multiplier(20) > NbaPredictor::mov_multiplier(5));
        assert!(NbaPredictor::mov_multiplier(5) > NbaPredictor::mov_multiplier(1));
    }

    #[test]
    fn mov_multiplier_cap() {
        // Should not exceed the hard cap regardless of blowout size
        assert!(NbaPredictor::mov_multiplier(100) <= 2.5);
    }

    // ── ensemble_weights ─────────────────────────────────────────────────────

    #[test]
    fn ensemble_weights_sum_to_one() {
        for &games in &[0, 10, 27, 55, 82] {
            let (a, b, c, d, e) = NbaPredictor::ensemble_weights(games, games);
            let total = a + b + c + d + e;
            assert!((total - 1.0).abs() < 1e-10, "weights must sum to 1.0 at {} games", games);
        }
    }

    #[test]
    fn ensemble_weights_elo_dominates_early() {
        let (w_nr, w_elo, _, w_ff, _) = NbaPredictor::ensemble_weights(0, 0);
        assert!(w_elo > w_nr, "ELO should dominate net rating early season");
        assert!(w_elo > w_ff, "ELO should dominate four factors early season");
    }

    #[test]
    fn ensemble_weights_net_rating_dominates_late() {
        let (w_nr, w_elo, _, _, _) = NbaPredictor::ensemble_weights(82, 82);
        assert!(w_nr > w_elo, "Net rating should dominate ELO late season");
    }

    // ── four_factors_model ───────────────────────────────────────────────────

    #[test]
    fn four_factors_equal_teams_home_advantage() {
        let avg_stats = NbaAdvancedStats {
            team_id: "x".into(), off_rating: 113.0, def_rating: 113.0, net_rating: 0.0,
            pace: 100.0, efg_pct: 0.52, opp_efg_pct: 0.52, tov_pct: 0.14,
            opp_tov_pct: 0.14, oreb_pct: 0.28, opp_oreb_pct: 0.28, ft_rate: 0.24,
            opp_ft_rate: 0.24, games_played: 55, wins: 28, season: "2025-26".into(),
            fetched_at: "2026-01-01T00:00:00Z".into(),
        };
        let p = NbaPredictor::new();
        let prob = p.four_factors_model(&avg_stats, &avg_stats);
        // Equal teams: only HCA logit offset should apply → ~57%
        assert!(prob > 0.55 && prob < 0.60, "equal teams four-factors should give ~57%, got {:.3}", prob);
    }

    // ── schedule_adjustment helpers ──────────────────────────────────────────

    #[tokio::test]
    async fn schedule_adjustment_no_history_returns_zero() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE matches (
                id TEXT, home_team_id TEXT, away_team_id TEXT,
                home_team_name TEXT, away_team_name TEXT,
                sport TEXT, league TEXT, match_date TEXT,
                status TEXT, home_score INTEGER, away_score INTEGER,
                created_at TEXT, updated_at TEXT
            )"
        )
        .execute(&pool)
        .await
        .unwrap();

        let m = Match {
            id: "m1".into(),
            home_team_id: "home".into(),
            away_team_id: "away".into(),
            home_team_name: "Home".into(),
            away_team_name: "Away".into(),
            sport: "basketball".into(),
            league: "NBA".into(),
            match_date: Utc::now(),
            status: "scheduled".into(),
            home_score: None,
            away_score: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let p = NbaPredictor::new();
        let delta = p.schedule_adjustment(&pool, &m).await.unwrap();
        assert_eq!(delta, 0.0, "no prior games should give zero schedule delta");
    }
}
