use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub sport: String, // "football" or "basketball"
    pub league: String, // "EPL", "Champions League", "NBA"
    pub logo_url: Option<String>,
    pub elo_rating: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Match {
    pub id: String,
    pub home_team_id: String,
    pub away_team_id: String,
    pub home_team_name: String,
    pub away_team_name: String,
    pub sport: String,
    pub league: String,
    pub match_date: DateTime<Utc>,
    pub status: String, // "scheduled", "live", "finished"
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Prediction {
    pub id: String,
    pub match_id: String,
    pub home_win_probability: f64,
    pub away_win_probability: f64,
    pub draw_probability: Option<f64>, // Only for football
    pub model_version: String,
    pub confidence_score: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TeamStats {
    pub id: String,
    pub team_id: String,
    pub season: String,
    pub matches_played: i32,
    pub wins: i32,
    pub draws: Option<i32>, // Only for football
    pub losses: i32,
    pub goals_for: Option<i32>, // Football
    pub goals_against: Option<i32>, // Football
    pub points_for: Option<i32>, // Basketball
    pub points_against: Option<i32>, // Basketball
    pub form: String, // Last 5 games: "WLWDW" etc
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingMatchWithPrediction {
    pub match_info: Match,
    pub prediction: Option<Prediction>,
    pub home_team_stats: Option<TeamStats>,
    pub away_team_stats: Option<TeamStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub match_id: String,
    pub match_info: Match,
    pub our_prediction: Prediction,
    pub market_home_odds: f64,
    pub market_away_odds: f64,
    pub market_draw_odds: Option<f64>,
    pub edge_value: f64,
    /// True when odds come from The Odds API, false when simulated
    pub is_live_odds: bool,
    pub bookmaker: Option<String>,
    pub odds_fetched_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOdds {
    pub match_id: String,
    pub bookmaker: String,
    pub home_odds: f64,
    pub draw_odds: Option<f64>,
    pub away_odds: f64,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRequest {
    pub sport: String,
    pub teams: Option<Vec<String>>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub stats_categories: Vec<String>, // "basic", "advanced", "form", etc.
    pub format: String, // "csv" or "json"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamProfile {
    pub team: Team,
    pub current_stats: TeamStats,
    pub recent_matches: Vec<Match>,
    pub elo_history: Vec<EloHistoryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EloHistoryPoint {
    pub team_id: String,
    pub date: DateTime<Utc>,
    pub elo_rating: f64,
    pub match_id: Option<String>,
}

// API Response types
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: Utc::now(),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            timestamp: Utc::now(),
        }
    }
}