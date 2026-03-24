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

/// Advanced per-team NBA stats fetched from stats.nba.com.
/// Stores Bayesian-friendly raw values; shrinkage is applied at prediction time.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NbaAdvancedStats {
    pub team_id: String,
    /// Offensive rating: points scored per 100 possessions
    pub off_rating: f64,
    /// Defensive rating: points allowed per 100 possessions
    pub def_rating: f64,
    /// Net rating: off_rating − def_rating
    pub net_rating: f64,
    /// Pace: possessions per 48 minutes
    pub pace: f64,
    // ── Four Factors (offense) ───────────────────────────────────────────────
    pub efg_pct: f64,      // Effective FG%
    pub opp_efg_pct: f64,  // Opponent eFG% (defensive proxy)
    pub tov_pct: f64,      // Turnover rate (lower is better for offense)
    pub opp_tov_pct: f64,  // Forced turnover rate (higher is better for defense)
    pub oreb_pct: f64,     // Offensive rebound %
    pub opp_oreb_pct: f64, // Opponent OREB% (defensive proxy)
    pub ft_rate: f64,      // Free throw rate = FTA/FGA
    pub opp_ft_rate: f64,  // Opponent FTr
    // ── Meta ─────────────────────────────────────────────────────────────────
    pub games_played: i32,
    pub wins: i32,
    pub season: String,
    pub fetched_at: String,
}

/// Per-component breakdown returned by the /matches/:id/analysis endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchAnalysis {
    pub match_id: String,
    pub home_team_name: String,
    pub away_team_name: String,
    pub sport: String,
    pub elo: EloComponent,
    pub form: FormComponent,
    pub h2h: H2hComponent,
    pub schedule: ScheduleComponent,
    pub model_version: String,
    pub final_home_prob: f64,
    pub final_away_prob: f64,
    pub draw_prob: Option<f64>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EloComponent {
    pub home_elo: f64,
    pub away_elo: f64,
    pub diff: f64,
    pub home_prob: f64,
    pub weight: f64,
    pub narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormComponent {
    pub home_avg_margin: f64,
    pub away_avg_margin: f64,
    pub home_games_used: i64,
    pub away_games_used: i64,
    pub home_prob: f64,
    pub weight: f64,
    pub narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct H2hComponent {
    pub home_wins: i64,
    pub away_wins: i64,
    pub draws: i64,
    pub total: i64,
    pub home_prob: f64,
    pub weight: f64,
    pub narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleComponent {
    pub home_rest_days: i64,
    pub away_rest_days: i64,
    pub away_on_back_to_back: bool,
    pub home_on_back_to_back: bool,
    pub away_consecutive_road: i64,
    pub adjustment: f64,
    pub narrative: String,
}

/// NBA player with season averages — one row per player per season.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NbaPlayerStats {
    pub player_id: i64,
    pub team_id: String,        // our internal ID: "nba_<bdl_id>"
    pub first_name: String,
    pub last_name: String,
    pub position: String,
    pub jersey_number: Option<String>,
    pub pts: f64,
    pub reb: f64,
    pub ast: f64,
    pub stl: f64,
    pub blk: f64,
    pub fg_pct: f64,
    pub fg3_pct: f64,
    pub min: String,            // avg minutes, e.g. "28.4"
    pub games_played: i32,
    pub season: String,
    pub fetched_at: String,
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

// ── ML model types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlEvaluation {
    pub model_name: String,
    pub fold: i32,
    pub year: i32,
    pub n_games: i32,
    pub brier_score: f64,
    pub log_loss: f64,
    pub accuracy: f64,
    pub evaluated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureContribution {
    pub feature_name: String,
    pub feature_value: f64,
    pub contribution: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreDistribution {
    pub match_id: String,
    pub p_home_win: f64,
    pub expected_margin: f64,
    /// 80 probability buckets: index i → margin = i-40
    pub buckets: Vec<f64>,
}