use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;
// use nalgebra::{DVector, DMatrix}; // For future advanced statistical models
// use statrs::distribution::{Normal, ContinuousCDF}; // For future probabilistic models

use crate::db::{get_team_by_id, insert_prediction, get_prediction_by_match_id};
use crate::models::{Match, Prediction, Team};
use crate::services::EloCalculator;

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

        // Calculate confidence score based on model agreement
        let confidence = self.calculate_confidence_score(
            (elo_home_prob, elo_away_prob, elo_draw_prob),
            (h2h_home_prob, h2h_away_prob, h2h_draw_prob),
            (form_home_prob, form_away_prob, form_draw_prob),
        );

        Ok(Prediction {
            id: Uuid::new_v4().to_string(),
            match_id: match_data.id.clone(),
            home_win_probability: normalized_home,
            away_win_probability: normalized_away,
            draw_probability: normalized_draw,
            model_version: "ensemble_v1.0".to_string(),
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

        // Apply some regression to the mean to avoid overconfidence
        let regression_factor = 0.3;
        let (default_home, default_away, default_draw) = self.league_average_prediction(sport)?;
        
        let adjusted_home = home_prob * (1.0 - regression_factor) + default_home * regression_factor;
        let adjusted_away = away_prob * (1.0 - regression_factor) + default_away * regression_factor;
        let adjusted_draw = match (draw_prob, default_draw) {
            (Some(draw), Some(def_draw)) => Some(draw * (1.0 - regression_factor) + def_draw * regression_factor),
            _ => None,
        };

        Ok((adjusted_home, adjusted_away, adjusted_draw))
    }

    /// Form-based prediction using recent team performance
    async fn form_based_prediction(&self,
        _pool: &SqlitePool,
        home_team: &Team,
        away_team: &Team,
        sport: &str,
    ) -> Result<(f64, f64, Option<f64>)> {
        // Simplified form-based prediction using ELO ratings as proxy for form
        // In a more sophisticated version, this would analyze recent match results, goals scored/conceded, etc.
        
        let elo_diff = home_team.elo_rating - away_team.elo_rating;
        
        // Convert ELO difference to win probability using sigmoid function
        let sigmoid = |x: f64| 1.0 / (1.0 + (-x / 200.0).exp());
        let home_prob_base = sigmoid(elo_diff + 50.0); // Home advantage
        
        match sport {
            "football" => {
                let draw_prob = 0.27; // Average draw rate in football
                let home_prob = home_prob_base * (1.0 - draw_prob);
                let away_prob = (1.0 - home_prob_base) * (1.0 - draw_prob);
                Ok((home_prob, away_prob, Some(draw_prob)))
            }
            _ => {
                Ok((home_prob_base, 1.0 - home_prob_base, None))
            }
        }
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

    /// Generate market edge analysis
    pub async fn find_market_edges(&self, pool: &SqlitePool) -> Result<Vec<crate::models::Edge>> {
        // This would integrate with betting APIs to compare our predictions with market odds
        // For now, we'll generate mock edges for demonstration
        
        let upcoming_matches = crate::db::get_upcoming_matches(pool, None).await?;
        let mut edges = Vec::new();

        for match_data in upcoming_matches {
            if let Some(our_prediction) = get_prediction_by_match_id(pool, &match_data.id).await? {
                // Mock market odds (in reality, these would come from betting APIs)
                let market_home_odds = self.probability_to_odds(our_prediction.home_win_probability + 0.1);
                let market_away_odds = self.probability_to_odds(our_prediction.away_win_probability - 0.05);
                let market_draw_odds = our_prediction.draw_probability.map(|p| self.probability_to_odds(p - 0.05));

                // Calculate edge value (Kelly criterion could be used here)
                let home_edge = self.calculate_edge_value(our_prediction.home_win_probability, market_home_odds);
                let away_edge = self.calculate_edge_value(our_prediction.away_win_probability, market_away_odds);
                
                let max_edge = home_edge.max(away_edge);

                if max_edge > 0.05 { // Only include significant edges
                    edges.push(crate::models::Edge {
                        match_id: match_data.id.clone(),
                        match_info: match_data,
                        our_prediction,
                        market_home_odds,
                        market_away_odds,
                        market_draw_odds,
                        edge_value: max_edge,
                    });
                }
            }
        }

        // Sort by edge value, highest first
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

    /// Calculate betting edge value
    fn calculate_edge_value(&self, our_probability: f64, market_odds: f64) -> f64 {
        let implied_market_probability = 1.0 / market_odds;
        (our_probability - implied_market_probability).max(0.0)
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