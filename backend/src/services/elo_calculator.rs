use anyhow::Result;
use sqlx::SqlitePool;
use chrono::Utc;

use crate::db::{get_team_by_id, insert_team};
use crate::models::{Team, Match};

pub struct EloCalculator {
    k_factor: f64,
}

impl EloCalculator {
    pub fn new() -> Self {
        Self {
            k_factor: 32.0, // Standard K-factor, can be adjusted
        }
    }

    /// Calculate expected score based on ELO ratings
    pub fn expected_score(rating_a: f64, rating_b: f64) -> f64 {
        1.0 / (1.0 + 10f64.powf((rating_b - rating_a) / 400.0))
    }

    /// Update ELO ratings after a match
    pub fn update_ratings(&self, 
        home_rating: f64, 
        away_rating: f64, 
        home_score: i32, 
        away_score: i32,
        is_neutral_venue: bool
    ) -> (f64, f64) {
        let home_advantage = if is_neutral_venue { 0.0 } else { 100.0 }; // Home advantage bonus
        let adjusted_home_rating = home_rating + home_advantage;
        
        let expected_home = Self::expected_score(adjusted_home_rating, away_rating);
        let expected_away = 1.0 - expected_home;
        
        let actual_home = match home_score.cmp(&away_score) {
            std::cmp::Ordering::Greater => 1.0, // Win
            std::cmp::Ordering::Equal => 0.5,   // Draw
            std::cmp::Ordering::Less => 0.0,    // Loss
        };
        let actual_away = 1.0 - actual_home;
        
        // Apply goal difference multiplier for more accurate ratings
        let goal_diff = (home_score - away_score).abs() as f64;
        let goal_multiplier = if goal_diff <= 1.0 {
            1.0
        } else if goal_diff == 2.0 {
            1.5
        } else {
            (11.0 + goal_diff) / 8.0
        };
        
        let new_home_rating = home_rating + self.k_factor * goal_multiplier * (actual_home - expected_home);
        let new_away_rating = away_rating + self.k_factor * goal_multiplier * (actual_away - expected_away);
        
        (new_home_rating, new_away_rating)
    }

    /// Calculate win probability based on ELO ratings
    pub fn win_probability(&self, home_rating: f64, away_rating: f64, sport: &str) -> (f64, f64, Option<f64>) {
        let home_advantage = 100.0; // Home advantage bonus
        let adjusted_home_rating = home_rating + home_advantage;
        
        let home_expected = Self::expected_score(adjusted_home_rating, away_rating);
        
        match sport {
            "football" => {
                // For football, we need to account for draws
                // Use a more sophisticated model that accounts for the nature of football
                let draw_probability = 0.25; // Base draw probability
                let home_win_prob = home_expected * (1.0 - draw_probability);
                let away_win_prob = (1.0 - home_expected) * (1.0 - draw_probability);
                
                (home_win_prob, away_win_prob, Some(draw_probability))
            }
            "basketball" => {
                // Basketball rarely has draws
                (home_expected, 1.0 - home_expected, None)
            }
            _ => {
                // Default to binary outcome
                (home_expected, 1.0 - home_expected, None)
            }
        }
    }

    /// Update team ELO ratings in database after match results
    pub async fn update_team_ratings(&self, pool: &SqlitePool, match_data: &Match) -> Result<()> {
        if match_data.status != "finished" || match_data.home_score.is_none() || match_data.away_score.is_none() {
            return Ok(()); // Skip if match not finished or scores not available
        }

        let home_score = match_data.home_score.unwrap();
        let away_score = match_data.away_score.unwrap();

        // Get current team ratings
        let home_team = get_team_by_id(pool, &match_data.home_team_id).await?
            .ok_or_else(|| anyhow::anyhow!("Home team not found"))?;
        let away_team = get_team_by_id(pool, &match_data.away_team_id).await?
            .ok_or_else(|| anyhow::anyhow!("Away team not found"))?;

        // Calculate new ratings
        let (new_home_rating, new_away_rating) = self.update_ratings(
            home_team.elo_rating,
            away_team.elo_rating,
            home_score,
            away_score,
            false, // Assume home venue advantage
        );

        let home_team_name = home_team.name.clone();
        let away_team_name = away_team.name.clone();
        let old_home_rating = home_team.elo_rating;
        let old_away_rating = away_team.elo_rating;

        // Update home team
        let updated_home_team = Team {
            elo_rating: new_home_rating,
            updated_at: Utc::now(),
            ..home_team
        };
        insert_team(pool, &updated_home_team).await?;

        // Update away team
        let updated_away_team = Team {
            elo_rating: new_away_rating,
            updated_at: Utc::now(),
            ..away_team
        };
        insert_team(pool, &updated_away_team).await?;

        tracing::info!(
            "Updated ELO ratings: {} ({:.1} -> {:.1}), {} ({:.1} -> {:.1})",
            home_team_name,
            old_home_rating,
            new_home_rating,
            away_team_name,
            old_away_rating,
            new_away_rating
        );

        Ok(())
    }

    /// Calculate ELO-based predictions for upcoming matches
    pub async fn calculate_predictions_for_matches(&self, pool: &SqlitePool, matches: &[Match]) -> Result<Vec<(String, f64, f64, Option<f64>)>> {
        let mut predictions = Vec::new();

        for match_data in matches {
            if match_data.status != "scheduled" {
                continue;
            }

            let home_team = get_team_by_id(pool, &match_data.home_team_id).await?;
            let away_team = get_team_by_id(pool, &match_data.away_team_id).await?;

            if let (Some(home_team), Some(away_team)) = (home_team, away_team) {
                let (home_win_prob, away_win_prob, draw_prob) = self.win_probability(
                    home_team.elo_rating,
                    away_team.elo_rating,
                    &match_data.sport,
                );

                predictions.push((
                    match_data.id.clone(),
                    home_win_prob,
                    away_win_prob,
                    draw_prob,
                ));
            }
        }

        Ok(predictions)
    }

    /// Initialize ELO ratings for new teams based on league strength
    pub fn initial_rating_for_league(league: &str) -> f64 {
        match league {
            "Champions League" => 1400.0, // Higher initial rating for elite competition
            "EPL" => 1300.0,              // High rating for top league
            "NBA" => 1200.0,              // Standard rating for NBA
            _ => 1200.0,                  // Default rating
        }
    }

    /// Adjust K-factor based on team strength and match importance
    pub fn adaptive_k_factor(&self, team_rating: f64, match_importance: f64) -> f64 {
        let base_k = self.k_factor;
        
        // Reduce K-factor for established teams (higher ratings)
        let rating_factor = if team_rating > 1600.0 {
            0.8
        } else if team_rating > 1400.0 {
            0.9
        } else {
            1.0
        };

        // Increase K-factor for important matches
        base_k * rating_factor * match_importance
    }

    /// Calculate team strength based on recent form and ELO
    pub fn team_strength(&self, elo_rating: f64, recent_form: Option<&str>) -> f64 {
        let mut strength = elo_rating;

        if let Some(form) = recent_form {
            let form_adjustment = self.calculate_form_adjustment(form);
            strength += form_adjustment;
        }

        strength
    }

    /// Calculate adjustment based on recent form (e.g., "WLWDW")
    fn calculate_form_adjustment(&self, form: &str) -> f64 {
        let mut adjustment: f64 = 0.0;
        let mut weight = 1.0;

        for result in form.chars().rev() { // Most recent first
            match result {
                'W' => adjustment += 20.0 * weight,
                'D' => adjustment += 10.0 * weight,
                'L' => adjustment -= 20.0 * weight,
                _ => {}
            }
            weight *= 0.8; // Diminishing weight for older results
        }

        adjustment.clamp(-100.0, 100.0) // Cap the adjustment
    }
}