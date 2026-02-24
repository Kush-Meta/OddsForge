use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Calculate the difference between two dates in days
pub fn days_between(date1: DateTime<Utc>, date2: DateTime<Utc>) -> i64 {
    (date2 - date1).num_days()
}

/// Convert a win/loss/draw record to a form string (e.g., "WLWDW")
pub fn results_to_form(results: &[(char, DateTime<Utc>)]) -> String {
    let mut form = String::new();
    let mut sorted_results = results.to_vec();
    sorted_results.sort_by(|a, b| b.1.cmp(&a.1)); // Most recent first
    
    for (result, _) in sorted_results.iter().take(5) {
        form.push(*result);
    }
    
    form
}

/// Calculate win percentage from wins, draws, and losses
pub fn calculate_win_percentage(wins: u32, draws: Option<u32>, losses: u32) -> f64 {
    let total_games = wins + losses + draws.unwrap_or(0);
    if total_games == 0 {
        return 0.0;
    }
    
    let points = wins * 3 + draws.unwrap_or(0); // Football scoring
    (points as f64) / ((total_games * 3) as f64) * 100.0
}

/// Convert probability to implied odds
pub fn probability_to_odds(probability: f64) -> f64 {
    if probability <= 0.0 || probability >= 1.0 {
        return 1000.0; // Very high odds for impossible/certain events
    }
    1.0 / probability
}

/// Convert odds to implied probability
pub fn odds_to_probability(odds: f64) -> f64 {
    if odds <= 1.0 {
        return 0.99; // Cap at 99%
    }
    (1.0 / odds).min(0.99)
}

/// Calculate Kelly criterion bet size
pub fn kelly_criterion(win_probability: f64, odds: f64) -> f64 {
    let b = odds - 1.0; // Net odds received on the wager
    let p = win_probability;
    let q = 1.0 - p;
    
    let kelly = (b * p - q) / b;
    kelly.max(0.0).min(0.25) // Cap at 25% of bankroll
}

/// Normalize probabilities to sum to 1.0
pub fn normalize_probabilities(probs: Vec<f64>) -> Vec<f64> {
    let sum: f64 = probs.iter().sum();
    if sum == 0.0 {
        return probs;
    }
    probs.iter().map(|p| p / sum).collect()
}

/// Calculate moving average
pub fn moving_average(values: &[f64], window: usize) -> Vec<f64> {
    if values.len() < window {
        return values.to_vec();
    }
    
    let mut result = Vec::new();
    for i in (window - 1)..values.len() {
        let sum: f64 = values[(i - window + 1)..=i].iter().sum();
        result.push(sum / window as f64);
    }
    result
}

/// Format large numbers with appropriate suffixes
pub fn format_number(num: f64) -> String {
    if num >= 1_000_000.0 {
        format!("{:.1}M", num / 1_000_000.0)
    } else if num >= 1_000.0 {
        format!("{:.1}K", num / 1_000.0)
    } else {
        format!("{:.0}", num)
    }
}

/// Validate team name format
pub fn validate_team_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 100
}

/// Validate league name format
pub fn validate_league_name(league: &str) -> bool {
    matches!(league, "EPL" | "Champions League" | "NBA" | "Premier League")
}

/// Calculate Elo rating change
pub fn calculate_elo_change(_rating: f64, expected_score: f64, actual_score: f64, k_factor: f64) -> f64 {
    k_factor * (actual_score - expected_score)
}

/// Simple hash function for generating consistent IDs
pub fn simple_hash(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchSummary {
    pub home_team: String,
    pub away_team: String,
    pub date: DateTime<Utc>,
    pub our_prediction: Option<PredictionSummary>,
    pub result: Option<MatchResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionSummary {
    pub home_win_prob: f64,
    pub away_win_prob: f64,
    pub draw_prob: Option<f64>,
    pub confidence: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchResult {
    pub home_score: i32,
    pub away_score: i32,
    pub winner: String, // "home", "away", "draw"
}

/// Calculate prediction accuracy
pub fn calculate_prediction_accuracy(predictions: &[MatchSummary]) -> f64 {
    let mut correct = 0;
    let mut total = 0;
    
    for match_summary in predictions {
        if let (Some(pred), Some(result)) = (&match_summary.our_prediction, &match_summary.result) {
            total += 1;
            
            let predicted_winner = if pred.home_win_prob > pred.away_win_prob {
                if let Some(draw_prob) = pred.draw_prob {
                    if pred.home_win_prob > draw_prob {
                        "home"
                    } else {
                        "draw"
                    }
                } else {
                    "home"
                }
            } else if pred.away_win_prob > pred.home_win_prob {
                if let Some(draw_prob) = pred.draw_prob {
                    if pred.away_win_prob > draw_prob {
                        "away"
                    } else {
                        "draw"
                    }
                } else {
                    "away"
                }
            } else {
                "draw"
            };
            
            if predicted_winner == result.winner {
                correct += 1;
            }
        }
    }
    
    if total == 0 {
        0.0
    } else {
        correct as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_probability_to_odds() {
        assert_eq!(probability_to_odds(0.5), 2.0);
        assert_eq!(probability_to_odds(0.25), 4.0);
        assert!(probability_to_odds(0.0) > 100.0);
    }

    #[test]
    fn test_odds_to_probability() {
        assert!((odds_to_probability(2.0) - 0.5).abs() < 0.001);
        assert!((odds_to_probability(4.0) - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_normalize_probabilities() {
        let probs = vec![0.4, 0.3, 0.2];
        let normalized = normalize_probabilities(probs);
        let sum: f64 = normalized.iter().sum();
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_calculate_win_percentage() {
        assert_eq!(calculate_win_percentage(3, Some(1), 1), 60.0); // 10 points out of 15 possible
        assert_eq!(calculate_win_percentage(0, None, 0), 0.0);
    }
}