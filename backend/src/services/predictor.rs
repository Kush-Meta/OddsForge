use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;
// use nalgebra::{DVector, DMatrix}; // For future advanced statistical models
// use statrs::distribution::{Normal, ContinuousCDF}; // For future probabilistic models

use crate::db::{get_team_by_id, insert_prediction, get_prediction_by_match_id, get_market_odds};
use crate::models::{Match, Prediction, Team};
use crate::services::EloCalculator;

/// Captures recent weighted performance for a team in a specific playing context (home or away).
struct RollingForm {
    /// Exponentially-weighted points rate, normalised to [0, 1].
    /// 1.0 = winning every game, 0.0 = losing every game.
    rate: f64,
    sample_size: usize,
}

pub struct PredictionEngine {
    elo_calculator: EloCalculator,
}

impl PredictionEngine {
    pub fn new() -> Self {
        Self {
            elo_calculator: EloCalculator::new(),
        }
    }

    /// Generate predictions for a list of matches using multiple models
    pub async fn generate_predictions(&self, pool: &SqlitePool, matches: &[Match]) -> Result<()> {
        for match_data in matches {
            if match_data.status != "scheduled" {
                continue;
            }

            let prediction = self.predict_match_outcome(pool, match_data).await?;
            insert_prediction(pool, &prediction).await?;
            
            tracing::info!(
                "Generated prediction for {} vs {}: Home {:.2}%, Away {:.2}%{}",
                match_data.home_team_name,
                match_data.away_team_name,
                prediction.home_win_probability * 100.0,
                prediction.away_win_probability * 100.0,
                prediction.draw_probability.map_or(String::new(), |d| format!(", Draw {:.2}%", d * 100.0))
            );
        }

        Ok(())
    }

    /// Predict match outcome using ensemble of models
    pub async fn predict_match_outcome(&self, pool: &SqlitePool, match_data: &Match) -> Result<Prediction> {
        let home_team = get_team_by_id(pool, &match_data.home_team_id).await?
            .ok_or_else(|| anyhow::anyhow!("Home team not found"))?;
        let away_team = get_team_by_id(pool, &match_data.away_team_id).await?
            .ok_or_else(|| anyhow::anyhow!("Away team not found"))?;

        // Model 1: ELO-based prediction
        let (elo_home_prob, elo_away_prob, elo_draw_prob) = self.elo_calculator.win_probability(
            home_team.elo_rating,
            away_team.elo_rating,
            &match_data.sport,
        );

        // Model 2: Head-to-head and form-based prediction
        let (h2h_home_prob, h2h_away_prob, h2h_draw_prob) = self.head_to_head_prediction(
            pool, &home_team, &away_team, &match_data.sport
        ).await?;

        // Model 3: Recent form prediction
        let (form_home_prob, form_away_prob, form_draw_prob) = self.form_based_prediction(
            pool, &home_team, &away_team, &match_data.sport
        ).await?;

        // Ensemble: Weighted average of models
        let elo_weight = 0.5;
        let h2h_weight = 0.3;
        let form_weight = 0.2;

        let final_home_prob = elo_home_prob * elo_weight + h2h_home_prob * h2h_weight + form_home_prob * form_weight;
        let final_away_prob = elo_away_prob * elo_weight + h2h_away_prob * h2h_weight + form_away_prob * form_weight;
        let final_draw_prob = match (elo_draw_prob, h2h_draw_prob, form_draw_prob) {
            (Some(elo_draw), Some(h2h_draw), Some(form_draw)) => {
                Some(elo_draw * elo_weight + h2h_draw * h2h_weight + form_draw * form_weight)
            }
            _ => None,
        };

        // Normalize probabilities to sum to 1
        let total = final_home_prob + final_away_prob + final_draw_prob.unwrap_or(0.0);
        let normalized_home = final_home_prob / total;
        let normalized_away = final_away_prob / total;
        let normalized_draw = final_draw_prob.map(|d| d / total);

        // NBA rest-day adjustment (compute once, reuse for both final probs and confidence).
        let rest_adj = if match_data.sport == "basketball" {
            self.rest_day_advantage(pool, match_data).await.unwrap_or(0.0)
        } else {
            0.0
        };

        let (final_home, final_away) = if rest_adj != 0.0 {
            let adj_home = (normalized_home + rest_adj).max(0.01);
            let adj_away = (normalized_away - rest_adj).max(0.01);
            let sum = adj_home + adj_away;
            (adj_home / sum, adj_away / sum)
        } else {
            (normalized_home, normalized_away)
        };

        // Confidence: blend prediction strength (primary) + model agreement (secondary).
        //
        // Old formula was inverted: strong ELO favourites disagreed with the league-average
        // H2H/form fallbacks → high std_dev → low confidence for strong predictions.
        // New formula: a decisive ensemble + agreeing models = high confidence.
        let home_probs = [elo_home_prob, h2h_home_prob, form_home_prob];
        let mean_hp = home_probs.iter().sum::<f64>() / 3.0;
        let std_dev = (home_probs.iter().map(|&p| (p - mean_hp).powi(2)).sum::<f64>() / 3.0).sqrt();
        let agreement = (1.0 - std_dev / 0.15).clamp(0.0, 1.0);

        let best_prob = final_home.max(final_away).max(normalized_draw.unwrap_or(0.0));
        let strength = ((best_prob - 0.5) * 2.5).clamp(0.0, 1.0);

        let confidence = (0.40_f64 + 0.35 * strength + 0.25 * agreement).clamp(0.40, 0.95);

        Ok(Prediction {
            id: Uuid::new_v4().to_string(),
            match_id: match_data.id.clone(),
            home_win_probability: final_home,
            away_win_probability: final_away,
            draw_probability: normalized_draw,
            model_version: "ensemble_v2.0".to_string(),
            confidence_score: confidence,
            created_at: Utc::now(),
        })
    }

    /// Head-to-head prediction based on historical matchups
    async fn head_to_head_prediction(&self, 
        pool: &SqlitePool, 
        home_team: &Team, 
        away_team: &Team,
        sport: &str
    ) -> Result<(f64, f64, Option<f64>)> {
        // Get historical matchups between these teams
        let h2h_matches = self.get_head_to_head_matches(pool, &home_team.id, &away_team.id).await?;
        
        if h2h_matches.is_empty() {
            // No historical data, fall back to league averages
            return self.league_average_prediction(sport);
        }

        let mut home_wins = 0;
        let mut away_wins = 0;
        let mut draws = 0;
        let mut total_matches = 0;

        for match_data in &h2h_matches {
            if let (Some(home_score), Some(away_score)) = (match_data.home_score, match_data.away_score) {
                total_matches += 1;
                match home_score.cmp(&away_score) {
                    std::cmp::Ordering::Greater => {
                        if match_data.home_team_id == home_team.id {
                            home_wins += 1;
                        } else {
                            away_wins += 1;
                        }
                    }
                    std::cmp::Ordering::Less => {
                        if match_data.away_team_id == away_team.id {
                            away_wins += 1;
                        } else {
                            home_wins += 1;
                        }
                    }
                    std::cmp::Ordering::Equal => draws += 1,
                }
            }
        }

        if total_matches == 0 {
            return self.league_average_prediction(sport);
        }

        let home_prob = home_wins as f64 / total_matches as f64;
        let away_prob = away_wins as f64 / total_matches as f64;
        let draw_prob = if sport == "football" {
            Some(draws as f64 / total_matches as f64)
        } else {
            None
        };

        // Regression to mean: scales down with sample size.
        // With 1 H2H match we regress 90%, with 10+ we regress ~30%.
        let regression_factor = (1.0 - (total_matches as f64).sqrt() / 4.0).clamp(0.30, 0.90);
        let (default_home, default_away, default_draw) = self.league_average_prediction(sport)?;
        
        let adjusted_home = home_prob * (1.0 - regression_factor) + default_home * regression_factor;
        let adjusted_away = away_prob * (1.0 - regression_factor) + default_away * regression_factor;
        let adjusted_draw = match (draw_prob, default_draw) {
            (Some(draw), Some(def_draw)) => Some(draw * (1.0 - regression_factor) + def_draw * regression_factor),
            _ => None,
        };

        Ok((adjusted_home, adjusted_away, adjusted_draw))
    }

    /// Form-based prediction using each team's real recent results from the database.
    ///
    /// Uses home team's last 8 HOME games and away team's last 8 AWAY games — the contextual
    /// split (home form vs away form) is more predictive than overall form.
    /// Results are exponentially decayed so the most recent game weighs most heavily.
    async fn form_based_prediction(&self,
        pool: &SqlitePool,
        home_team: &Team,
        away_team: &Team,
        sport: &str,
    ) -> Result<(f64, f64, Option<f64>)> {
        let home_form = self.rolling_form(pool, &home_team.id, true, sport).await?;
        let away_form = self.rolling_form(pool, &away_team.id, false, sport).await?;

        // Not enough real data yet — fall back to league average
        if home_form.sample_size < 3 || away_form.sample_size < 3 {
            return self.league_average_prediction(sport);
        }

        // form_diff ∈ [-1, 1]: positive = home team in better contextual form
        let form_diff = home_form.rate - away_form.rate;
        // 0.30 home-field bonus keeps this consistent with the ELO model's +100-pt boost
        let adjusted = form_diff + 0.30;
        let home_prob_base = 1.0 / (1.0 + (-adjusted * 3.0).exp());

        match sport {
            "football" => {
                let competitiveness = 1.0 - (home_prob_base - 0.5).abs() * 2.0;
                let draw_prob = (0.10 + 0.22 * competitiveness).clamp(0.05, 0.35);
                Ok((
                    home_prob_base * (1.0 - draw_prob),
                    (1.0 - home_prob_base) * (1.0 - draw_prob),
                    Some(draw_prob),
                ))
            }
            _ => Ok((home_prob_base, 1.0 - home_prob_base, None)),
        }
    }

    /// Compute exponentially-weighted recent form for a team in a specific playing context.
    ///
    /// `home_context = true`  → query only games the team played at home
    /// `home_context = false` → query only games the team played away
    async fn rolling_form(
        &self,
        pool: &SqlitePool,
        team_id: &str,
        home_context: bool,
        sport: &str,
    ) -> Result<RollingForm> {
        let matches: Vec<Match> = if home_context {
            sqlx::query_as::<_, Match>(
                "SELECT * FROM matches
                 WHERE home_team_id = ? AND status = 'finished'
                   AND home_score IS NOT NULL AND sport = ?
                 ORDER BY match_date DESC LIMIT 8",
            )
            .bind(team_id)
            .bind(sport)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as::<_, Match>(
                "SELECT * FROM matches
                 WHERE away_team_id = ? AND status = 'finished'
                   AND away_score IS NOT NULL AND sport = ?
                 ORDER BY match_date DESC LIMIT 8",
            )
            .bind(team_id)
            .bind(sport)
            .fetch_all(pool)
            .await?
        };

        if matches.is_empty() {
            return Ok(RollingForm { rate: 0.5, sample_size: 0 });
        }

        // Max points per game: 3 (football W) or 1 (basketball W)
        let max_pts = if sport == "football" { 3.0_f64 } else { 1.0_f64 };
        let mut weighted_pts = 0.0_f64;
        let mut weight_total = 0.0_f64;

        for (i, m) in matches.iter().enumerate() {
            // Rows are newest-first (ORDER BY DESC), so i=0 is the most recent match.
            let decay = 0.85_f64.powi(i as i32);

            let pts = match (m.home_score, m.away_score) {
                (Some(hs), Some(as_)) => {
                    if home_context {
                        if hs > as_ { max_pts }
                        else if hs == as_ && sport == "football" { 1.0 }
                        else { 0.0 }
                    } else {
                        if as_ > hs { max_pts }
                        else if as_ == hs && sport == "football" { 1.0 }
                        else { 0.0 }
                    }
                }
                _ => max_pts * 0.5, // unknown score → assume average
            };

            weighted_pts += decay * pts;
            weight_total += decay * max_pts;
        }

        let rate = if weight_total > 0.0 { weighted_pts / weight_total } else { 0.5 };
        Ok(RollingForm { rate, sample_size: matches.len() })
    }

    /// Get league average probabilities
    fn league_average_prediction(&self, sport: &str) -> Result<(f64, f64, Option<f64>)> {
        match sport {
            "football" => {
                // Typical football statistics
                Ok((0.46, 0.27, Some(0.27))) // Home win, Away win, Draw
            }
            "basketball" => {
                // Basketball with home court advantage
                Ok((0.55, 0.45, None))
            }
            _ => {
                Ok((0.50, 0.50, None))
            }
        }
    }

    /// Get historical head-to-head matches
    async fn get_head_to_head_matches(&self, pool: &SqlitePool, team1_id: &str, team2_id: &str) -> Result<Vec<Match>> {
        let rows = sqlx::query_as::<_, Match>(
            r#"
            SELECT * FROM matches 
            WHERE ((home_team_id = ? AND away_team_id = ?) 
                OR (home_team_id = ? AND away_team_id = ?))
                AND status = 'finished'
            ORDER BY match_date DESC 
            LIMIT 10
            "#
        )
        .bind(team1_id)
        .bind(team2_id)
        .bind(team2_id)
        .bind(team1_id)
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    /// Calculate confidence score based on model agreement
    fn calculate_confidence_score(&self, 
        elo_probs: (f64, f64, Option<f64>),
        h2h_probs: (f64, f64, Option<f64>),
        form_probs: (f64, f64, Option<f64>),
    ) -> f64 {
        let models = vec![
            vec![elo_probs.0, elo_probs.1],
            vec![h2h_probs.0, h2h_probs.1],
            vec![form_probs.0, form_probs.1],
        ];

        // Calculate standard deviation across models for home win probability
        let home_probs: Vec<f64> = models.iter().map(|m| m[0]).collect();
        let mean_home = home_probs.iter().sum::<f64>() / home_probs.len() as f64;
        let variance = home_probs.iter().map(|&p| (p - mean_home).powi(2)).sum::<f64>() / home_probs.len() as f64;
        let std_dev = variance.sqrt();

        // Higher standard deviation = lower confidence
        // Map std_dev (0.0 to ~0.2) to confidence (1.0 to 0.5)
        let confidence = (1.0 - (std_dev / 0.2).min(0.5)).max(0.5);
        
        confidence
    }

    /// Generate market edge analysis using real odds from The Odds API.
    ///
    /// Simulated odds are deliberately NOT used as a fallback — the previous simulated
    /// formula (market = our_prob ± fixed offset) had zero overround after devigging,
    /// which made every match show an identical 5% edge regardless of teams.
    /// Real edges only exist when we have genuine market disagreement.
    pub async fn find_market_edges(&self, pool: &SqlitePool) -> Result<Vec<crate::models::Edge>> {
        let upcoming_matches = crate::db::get_upcoming_matches(pool, None).await?;
        let mut edges = Vec::new();

        for match_data in upcoming_matches {
            let Some(our_prediction) = get_prediction_by_match_id(pool, &match_data.id).await? else {
                continue;
            };

            // Skip if no real market odds in DB yet
            let Some(live) = get_market_odds(pool, &match_data.id).await.ok().flatten() else {
                continue;
            };

            // Devig: remove bookmaker overround to get true implied probabilities
            let (implied_home, implied_draw, implied_away) =
                devig(live.home_odds, live.draw_odds, live.away_odds);

            // Edge = our probability − devigged market probability (positive = value bet)
            let home_edge = our_prediction.home_win_probability - implied_home;
            let away_edge = our_prediction.away_win_probability - implied_away;
            let draw_edge = match (our_prediction.draw_probability, implied_draw) {
                (Some(ours), Some(mkt)) => ours - mkt,
                _ => 0.0,
            };

            let max_edge = home_edge.max(away_edge).max(draw_edge);

            if max_edge > 0.03 {
                edges.push(crate::models::Edge {
                    match_id: match_data.id.clone(),
                    match_info: match_data,
                    our_prediction,
                    market_home_odds: live.home_odds,
                    market_away_odds: live.away_odds,
                    market_draw_odds: live.draw_odds,
                    edge_value: max_edge,
                    is_live_odds: true,
                    bookmaker: Some(live.bookmaker),
                    odds_fetched_at: Some(live.fetched_at),
                });
            }
        }

        edges.sort_by(|a, b| b.edge_value.partial_cmp(&a.edge_value).unwrap_or(std::cmp::Ordering::Equal));
        Ok(edges)
    }

    /// Convert probability to decimal odds
    fn probability_to_odds(&self, probability: f64) -> f64 {
        if probability <= 0.0 {
            100.0 // Very high odds for near-impossible events
        } else {
            1.0 / probability
        }
    }

    /// Calculate betting edge value (kept for simulated-odds path)
    fn calculate_edge_value(&self, our_probability: f64, market_odds: f64) -> f64 {
        let implied_market_probability = 1.0 / market_odds;
        (our_probability - implied_market_probability).max(0.0)
    }

    /// NBA rest-day advantage: returns a probability delta for the home team.
    ///
    /// Positive = home team is better rested; negative = away team is better rested.
    /// Literature: each net rest day is worth ~2.5 pp in NBA win probability (capped at ±3 days).
    async fn rest_day_advantage(&self, pool: &SqlitePool, match_data: &Match) -> Result<f64> {
        let home_rest = self.days_rest(pool, &match_data.home_team_id, match_data.match_date).await?;
        let away_rest = self.days_rest(pool, &match_data.away_team_id, match_data.match_date).await?;
        // unwrap_or(3): if no prior game found, assume well-rested
        let net = (home_rest.unwrap_or(3) as i64 - away_rest.unwrap_or(3) as i64).clamp(-3, 3);
        Ok(net as f64 * 0.025)
    }

    /// Returns the number of rest days a team has before `upcoming_date`.
    /// Defined as (days since their last finished game) − 1, capped at 7.
    async fn days_rest(
        &self,
        pool: &SqlitePool,
        team_id: &str,
        upcoming_date: DateTime<Utc>,
    ) -> Result<Option<u32>> {
        let last = sqlx::query_as::<_, Match>(
            "SELECT * FROM matches
             WHERE (home_team_id = ? OR away_team_id = ?)
               AND status = 'finished'
               AND match_date < ?
             ORDER BY match_date DESC LIMIT 1",
        )
        .bind(team_id)
        .bind(team_id)
        .bind(upcoming_date)
        .fetch_optional(pool)
        .await?;

        Ok(last.map(|m| {
            let days = (upcoming_date - m.match_date).num_days().max(0) as u32;
            days.saturating_sub(1).min(7)
        }))
    }

    /// Advanced statistical model using logistic regression
    pub async fn logistic_regression_prediction(&self,
        pool: &SqlitePool,
        home_team: &Team,
        away_team: &Team,
    ) -> Result<f64> {
        // Collect features for logistic regression
        let features = self.collect_team_features(pool, home_team, away_team).await?;
        
        // For demonstration, use a simple linear model
        // In practice, you'd train this on historical data
        let coefficients = vec![0.5, -0.3, 0.2, 0.1]; // Mock coefficients
        
        let mut linear_combination = 0.0;
        for (i, &feature) in features.iter().enumerate() {
            if i < coefficients.len() {
                linear_combination += coefficients[i] * feature;
            }
        }
        
        // Apply sigmoid function
        let probability = 1.0 / (1.0 + (-linear_combination).exp());
        
        Ok(probability)
    }

    /// Collect features for machine learning models
    async fn collect_team_features(&self,
        _pool: &SqlitePool,
        home_team: &Team,
        away_team: &Team,
    ) -> Result<Vec<f64>> {
        // Feature engineering - in practice, you'd collect many more features
        let features = vec![
            (home_team.elo_rating - away_team.elo_rating) / 100.0, // Normalized ELO difference
            1.0, // Home advantage (binary feature)
            home_team.elo_rating / 1000.0, // Normalized home team strength
            away_team.elo_rating / 1000.0, // Normalized away team strength
        ];

        Ok(features)
    }
}

/// Remove bookmaker overround from decimal odds, returning true implied probabilities.
/// Works for both 2-outcome (basketball) and 3-outcome (football) markets.
fn devig(home_odds: f64, draw_odds: Option<f64>, away_odds: f64) -> (f64, Option<f64>, f64) {
    let h = if home_odds > 0.0 { 1.0 / home_odds } else { 0.0 };
    let d = draw_odds.map(|x| if x > 0.0 { 1.0 / x } else { 0.0 });
    let a = if away_odds > 0.0 { 1.0 / away_odds } else { 0.0 };
    let total = h + d.unwrap_or(0.0) + a;
    if total <= 0.0 {
        return (0.5, draw_odds.map(|_| 0.25), 0.5);
    }
    (h / total, d.map(|x| x / total), a / total)
}