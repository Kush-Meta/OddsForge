use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use std::collections::HashMap;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

use crate::db::{
    clear_all_data, create_pool, get_all_teams, get_elo_history, get_finished_matches_ordered,
    get_team_by_id, get_team_current_stats, get_team_recent_matches, get_teams_by_league,
    get_upcoming_matches, get_prediction_by_match_id, init_database_with_pool, insert_elo_history,
    seed_data,
};
use crate::models::{ApiResponse, DatasetRequest, UpcomingMatchWithPrediction, TeamProfile, Team};
use crate::services::{DataFetcher, EloCalculator, PredictionEngine};

pub async fn serve(port: u16) -> anyhow::Result<()> {
    let pool = create_pool().await?;
    init_database_with_pool(&pool).await?;

    // ── HTTP server starts immediately ───────────────────────────────────────
    let app = create_router().with_state(pool.clone());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("OddsForge API server listening on port {}", port);

    // ── Initial data load + scheduler both run in background ─────────────────
    let init_pool = pool.clone();
    tokio::spawn(async move {
        let team_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teams")
            .fetch_one(&init_pool).await.unwrap_or(0);

        if team_count == 0 {
            let fetcher = DataFetcher::new();
            if fetcher.has_football_key() || fetcher.has_nba_key() {
                tracing::info!("API keys detected — fetching real data in background…");
                if let Err(e) = fetcher.fetch_all_data(&init_pool).await {
                    tracing::error!("Initial fetch failed: {}. Seeding fallback.", e);
                    let _ = seed_data(&init_pool).await;
                } else {
                    rebuild_elo(&init_pool).await;
                    compute_season_stats(&init_pool).await;
                }
            } else {
                tracing::info!("No API keys — seeding with sample data");
                let _ = seed_data(&init_pool).await;
            }
        }

        // Always regenerate predictions on startup so model changes take effect immediately
        refresh_predictions(&init_pool).await;

        // After initial load, hand off to the recurring scheduler
        background_scheduler(init_pool).await;
    });

    axum::serve(listener, app).await?;
    Ok(())
}

// ── Background scheduler ─────────────────────────────────────────────────────
//
// Rate limits:
//   football-data.org free  →  10 req / min
//   balldontlie.io free     →  ~60 req / min
//
// Schedule (per 60-second tick):
//   Every tick  : EPL matches (1 req) + NBA recent games (1–3 req)
//   Every 10 min: EPL teams (1 req) + NBA teams (1 req)
//   After fetch : rebuild ELO → regenerate predictions
//
async fn background_scheduler(pool: SqlitePool) {
    // Stagger first run by 5 s so startup logs are readable
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    let mut cycle: u32 = 0;

    loop {
        interval.tick().await;
        cycle += 1;
        tracing::info!("🔄  Background refresh cycle {}", cycle);

        let fetcher = DataFetcher::new();

        // ── football-data.org ────────────────────────────────────────────────
        if fetcher.has_football_key() {
            // Every tick: EPL match statuses / scores  (1 req)
            if let Err(e) = fetcher.fetch_epl_matches(&pool).await {
                tracing::error!("EPL match refresh failed: {}", e);
            }

            // Every 10 cycles (~10 min): also refresh team list  (1 req)
            if cycle % 10 == 0 {
                // Wait 6 s to stay inside 10-req/min window
                tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
                if let Err(e) = fetcher.fetch_epl_teams(&pool).await {
                    tracing::error!("EPL team refresh failed: {}", e);
                }
            }
        }

        // ── balldontlie.io ───────────────────────────────────────────────────
        if fetcher.has_nba_key() {
            // Every tick: last 3 days of NBA games  (1–3 req)
            if let Err(e) = fetcher.fetch_recent_nba_games(&pool, 3).await {
                tracing::error!("NBA recent-game refresh failed: {}", e);
            }

            // Every 10 cycles (~10 min): also refresh team list  (1 req)
            if cycle % 10 == 0 {
                if let Err(e) = fetcher.fetch_nba_teams(&pool).await {
                    tracing::error!("NBA team refresh failed: {}", e);
                }
            }
        }

        // ── Post-fetch: ELO + stats + predictions ────────────────────────────
        rebuild_elo(&pool).await;
        compute_season_stats(&pool).await;
        refresh_predictions(&pool).await;
    }
}

/// Reset all team ELOs to 1200 then replay every finished match in chronological order,
/// recording an elo_history point after each match for both teams.
async fn rebuild_elo(pool: &SqlitePool) {
    // Clear old history and reset ratings
    let _ = sqlx::query("DELETE FROM elo_history").execute(pool).await;
    if let Err(e) = sqlx::query("UPDATE teams SET elo_rating = 1200.0").execute(pool).await {
        tracing::error!("ELO reset failed: {}", e);
        return;
    }

    let matches = match get_finished_matches_ordered(pool).await {
        Ok(m) => m,
        Err(e) => { tracing::error!("Could not load finished matches: {}", e); return; }
    };

    let calc = EloCalculator::new();
    let mut updated = 0u32;

    for m in &matches {
        if calc.update_team_ratings(pool, m).await.is_err() {
            continue;
        }
        // Record ELO history for both teams after this match
        if let Ok(Some(ht)) = get_team_by_id(pool, &m.home_team_id).await {
            let _ = insert_elo_history(pool, &ht.id, m.match_date, ht.elo_rating, &m.id).await;
        }
        if let Ok(Some(at)) = get_team_by_id(pool, &m.away_team_id).await {
            let _ = insert_elo_history(pool, &at.id, m.match_date, at.elo_rating, &m.id).await;
        }
        updated += 1;
    }
    tracing::info!("ELO rebuilt from {} finished matches", updated);
}

/// Compute W/D/L, goals/points, and recent form for every team from real match data,
/// then upsert into team_stats.
async fn compute_season_stats(pool: &SqlitePool) {
    // Football stats
    let football_sql = r#"
        SELECT team_id, sport, SUM(played) as mp,
               SUM(wins) as w, SUM(draws) as d, SUM(losses) as l,
               SUM(gf) as gf, SUM(ga) as ga
        FROM (
            SELECT home_team_id as team_id, sport,
                   COUNT(*) as played,
                   SUM(CASE WHEN home_score > away_score THEN 1 ELSE 0 END) as wins,
                   SUM(CASE WHEN home_score = away_score THEN 1 ELSE 0 END) as draws,
                   SUM(CASE WHEN home_score < away_score THEN 1 ELSE 0 END) as losses,
                   SUM(home_score) as gf, SUM(away_score) as ga
            FROM matches WHERE status = 'finished' AND home_score IS NOT NULL AND sport = 'football'
            GROUP BY home_team_id, sport
            UNION ALL
            SELECT away_team_id, sport,
                   COUNT(*),
                   SUM(CASE WHEN away_score > home_score THEN 1 ELSE 0 END),
                   SUM(CASE WHEN away_score = home_score THEN 1 ELSE 0 END),
                   SUM(CASE WHEN away_score < home_score THEN 1 ELSE 0 END),
                   SUM(away_score), SUM(home_score)
            FROM matches WHERE status = 'finished' AND away_score IS NOT NULL AND sport = 'football'
            GROUP BY away_team_id, sport
        ) GROUP BY team_id, sport
    "#;

    // Basketball stats (no draws)
    let basketball_sql = r#"
        SELECT team_id, sport, SUM(played) as mp,
               SUM(wins) as w, 0 as d, SUM(losses) as l,
               SUM(pf) as pf, SUM(pa) as pa
        FROM (
            SELECT home_team_id as team_id, sport,
                   COUNT(*) as played,
                   SUM(CASE WHEN home_score > away_score THEN 1 ELSE 0 END) as wins,
                   SUM(CASE WHEN home_score < away_score THEN 1 ELSE 0 END) as losses,
                   SUM(home_score) as pf, SUM(away_score) as pa
            FROM matches WHERE status = 'finished' AND home_score IS NOT NULL AND sport = 'basketball'
            GROUP BY home_team_id, sport
            UNION ALL
            SELECT away_team_id, sport,
                   COUNT(*),
                   SUM(CASE WHEN away_score > home_score THEN 1 ELSE 0 END),
                   SUM(CASE WHEN away_score < home_score THEN 1 ELSE 0 END),
                   SUM(away_score), SUM(home_score)
            FROM matches WHERE status = 'finished' AND away_score IS NOT NULL AND sport = 'basketball'
            GROUP BY away_team_id, sport
        ) GROUP BY team_id, sport
    "#;

    for (sql, is_football) in [(football_sql, true), (basketball_sql, false)] {
        let rows = match sqlx::query(sql).fetch_all(pool).await {
            Ok(r) => r,
            Err(e) => { tracing::error!("Season stats query failed: {}", e); continue; }
        };

        for row in rows {
            let team_id: String = row.get("team_id");
            let mp: i64 = row.get("mp");
            let w: i64  = row.get("w");
            let d: i64  = row.get("d");
            let l: i64  = row.get("l");
            let stat1: i64 = if is_football { row.get("gf") } else { row.get("pf") };
            let stat2: i64 = if is_football { row.get("ga") } else { row.get("pa") };

            // Compute last-5 form string from most recent matches
            let form = recent_form(pool, &team_id, is_football).await;

            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            let _ = sqlx::query(
                r#"INSERT OR REPLACE INTO team_stats
                   (id, team_id, season, matches_played, wins, draws, losses,
                    goals_for, goals_against, points_for, points_against, form, updated_at)
                   VALUES (?, ?, '2025-26', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(&id)
            .bind(&team_id)
            .bind(mp as i32)
            .bind(w as i32)
            .bind(if is_football { Some(d as i32) } else { None::<i32> })
            .bind(l as i32)
            .bind(if is_football { Some(stat1 as i32) } else { None::<i32> })
            .bind(if is_football { Some(stat2 as i32) } else { None::<i32> })
            .bind(if !is_football { Some(stat1 as i32) } else { None::<i32> })
            .bind(if !is_football { Some(stat2 as i32) } else { None::<i32> })
            .bind(&form)
            .bind(&now)
            .execute(pool)
            .await;
        }
    }
    tracing::info!("Season stats computed for all teams");
}

/// Last 5 results as a string like "WWDLW" (football) or "WWLLW" (basketball).
async fn recent_form(pool: &SqlitePool, team_id: &str, is_football: bool) -> String {
    let rows = sqlx::query(
        r#"SELECT home_team_id, home_score, away_score
           FROM matches
           WHERE (home_team_id = ? OR away_team_id = ?) AND status = 'finished' AND home_score IS NOT NULL
           ORDER BY match_date DESC LIMIT 5"#,
    )
    .bind(team_id)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.iter().map(|row| {
        let is_home = row.get::<String, _>("home_team_id") == team_id;
        let hs: i32 = row.get("home_score");
        let aws: i32 = row.get("away_score");
        let (ts, os) = if is_home { (hs, aws) } else { (aws, hs) };
        if ts > os { 'W' }
        else if ts < os { 'L' }
        else if is_football { 'D' }
        else { 'L' }
    }).collect()
}

/// Generate / refresh predictions for all upcoming matches.
async fn refresh_predictions(pool: &SqlitePool) {
    let engine = PredictionEngine::new();
    match get_upcoming_matches(pool, None).await {
        Ok(matches) if !matches.is_empty() => {
            if let Err(e) = engine.generate_predictions(pool, &matches).await {
                tracing::error!("Prediction generation failed: {}", e);
            } else {
                tracing::info!("Predictions refreshed for {} matches", matches.len());
            }
        }
        Ok(_) => tracing::info!("No upcoming matches to predict"),
        Err(e) => tracing::error!("Failed to fetch upcoming matches: {}", e),
    }
}

fn create_router() -> Router<SqlitePool> {
    Router::new()
        .route("/health", get(health_check))
        .route("/matches/upcoming", get(get_upcoming_matches_handler))
        .route("/teams", get(get_all_teams_handler))
        .route("/teams/league/{sport}/{league}", get(get_teams_by_league_handler))
        .route("/teams/{id}/stats", get(get_team_stats_handler))
        .route("/predictions/edges", get(get_prediction_edges_handler))
        .route("/datasets/generate", post(generate_dataset_handler))
        .route("/data/fetch", post(fetch_data_handler))
        .route("/data/refresh", post(refresh_all_data_handler))
        .route("/predictions/generate", post(generate_predictions_handler))
        // Serve generated export files (CSV / JSON) from the exports directory
        .nest_service("/downloads", ServeDir::new("../data/exports"))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
        )
}

// Health check endpoint
async fn health_check() -> Json<ApiResponse<&'static str>> {
    Json(ApiResponse::success("OddsForge API is running"))
}

// GET /matches/upcoming - Get upcoming matches with predictions
#[derive(Deserialize)]
struct UpcomingMatchesQuery {
    sport: Option<String>,
    limit: Option<usize>,
}

async fn get_upcoming_matches_handler(
    State(pool): State<SqlitePool>,
    Query(params): Query<UpcomingMatchesQuery>,
) -> Result<Json<ApiResponse<Vec<UpcomingMatchWithPrediction>>>, StatusCode> {
    match get_upcoming_matches(&pool, params.sport.as_deref()).await {
        Ok(matches) => {
            let mut matches_with_predictions = Vec::new();
            let limit = params.limit.unwrap_or(50).min(100); // Cap at 100
            
            for match_data in matches.into_iter().take(limit) {
                let prediction = get_prediction_by_match_id(&pool, &match_data.id).await.ok().flatten();
                
                matches_with_predictions.push(UpcomingMatchWithPrediction {
                    match_info: match_data,
                    prediction,
                    home_team_stats: None, // TODO: Implement team stats fetching
                    away_team_stats: None,
                });
            }
            
            Ok(Json(ApiResponse::success(matches_with_predictions)))
        }
        Err(e) => {
            tracing::error!("Failed to fetch upcoming matches: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /teams - List all teams
async fn get_all_teams_handler(
    State(pool): State<SqlitePool>,
) -> Result<Json<ApiResponse<Vec<Team>>>, StatusCode> {
    match get_all_teams(&pool).await {
        Ok(teams) => Ok(Json(ApiResponse::success(teams))),
        Err(e) => {
            tracing::error!("Failed to fetch teams: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /teams/:id/stats - Get team analytics
async fn get_team_stats_handler(
    State(pool): State<SqlitePool>,
    Path(team_id): Path<String>,
) -> Result<Json<ApiResponse<TeamProfile>>, StatusCode> {
    match get_team_by_id(&pool, &team_id).await {
        Ok(Some(team)) => {
            let current_stats = get_team_current_stats(&pool, &team_id)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| crate::models::TeamStats {
                    id: uuid::Uuid::new_v4().to_string(),
                    team_id: team_id.clone(),
                    season: "2025-26".to_string(),
                    matches_played: 0,
                    wins: 0,
                    draws: Some(0),
                    losses: 0,
                    goals_for: Some(0),
                    goals_against: Some(0),
                    points_for: Some(0),
                    points_against: Some(0),
                    form: String::new(),
                    updated_at: chrono::Utc::now(),
                });

            let recent_matches = get_team_recent_matches(&pool, &team_id, 8)
                .await
                .unwrap_or_default();

            let elo_history = get_elo_history(&pool, &team_id)
                .await
                .unwrap_or_default();

            let profile = TeamProfile {
                team,
                current_stats,
                recent_matches,
                elo_history,
            };

            Ok(Json(ApiResponse::success(profile)))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch team stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /teams/league/:sport/:league - Get teams by league
async fn get_teams_by_league_handler(
    State(pool): State<SqlitePool>,
    Path((sport, league)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Vec<Team>>>, StatusCode> {
    match get_teams_by_league(&pool, &sport, &league).await {
        Ok(teams) => Ok(Json(ApiResponse::success(teams))),
        Err(e) => {
            tracing::error!("Failed to fetch teams by league: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /predictions/edges - Get market edge opportunities
async fn get_prediction_edges_handler(
    State(pool): State<SqlitePool>,
) -> Result<Json<ApiResponse<Vec<crate::models::Edge>>>, StatusCode> {
    let prediction_engine = PredictionEngine::new();
    
    match prediction_engine.find_market_edges(&pool).await {
        Ok(edges) => Ok(Json(ApiResponse::success(edges))),
        Err(e) => {
            tracing::error!("Failed to find market edges: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// POST /datasets/generate - Generate custom dataset
#[derive(Serialize)]
struct DatasetResponse {
    download_url: String,
    format: String,
    rows: usize,
    generated_at: chrono::DateTime<chrono::Utc>,
}

async fn generate_dataset_handler(
    State(pool): State<SqlitePool>,
    Json(request): Json<DatasetRequest>,
) -> Result<Json<ApiResponse<DatasetResponse>>, StatusCode> {
    match generate_custom_dataset(&pool, request).await {
        Ok(response) => Ok(Json(ApiResponse::success(response))),
        Err(e) => {
            tracing::error!("Failed to generate dataset: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// POST /data/fetch - Fetch sports data from APIs
#[derive(Deserialize)]
struct FetchDataRequest {
    sport: Option<String>,
    force_refresh: Option<bool>,
}

async fn fetch_data_handler(
    State(pool): State<SqlitePool>,
    Json(request): Json<FetchDataRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let fetcher = DataFetcher::new();
    
    let result: anyhow::Result<&str> = match request.sport.as_deref() {
        Some("football") => {
            fetcher.fetch_epl_teams(&pool).await
                .and(fetcher.fetch_epl_matches(&pool).await)
                .map(|_| "Football data fetched successfully")
        }
        Some("basketball") => {
            fetcher.fetch_nba_teams(&pool).await
                .and(fetcher.fetch_nba_games(&pool).await)
                .map(|_| "Basketball data fetched successfully")
        }
        _ => fetcher.fetch_all_data(&pool).await.map(|_| "All sports data fetched successfully"),
    };

    match result {
        Ok(message) => Ok(Json(ApiResponse::success(message.to_string()))),
        Err(e) => {
            tracing::error!("Failed to fetch data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// POST /data/refresh - Wipe and re-fetch all real data, then rebuild ELO + predictions
async fn refresh_all_data_handler(
    State(pool): State<SqlitePool>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    tracing::info!("Manual /data/refresh triggered");

    let fetcher = DataFetcher::new();
    if !fetcher.has_football_key() && !fetcher.has_nba_key() {
        return Ok(Json(ApiResponse::success(
            "No API keys configured — set FOOTBALL_DATA_API_KEY and/or BALLDONTLIE_API_KEY".to_string()
        )));
    }

    if let Err(e) = clear_all_data(&pool).await {
        tracing::error!("Clear failed: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    if let Err(e) = fetcher.fetch_all_data(&pool).await {
        tracing::error!("Fetch failed: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    rebuild_elo(&pool).await;
    compute_season_stats(&pool).await;
    refresh_predictions(&pool).await;

    let team_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teams")
        .fetch_one(&pool).await.unwrap_or(0);
    let match_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM matches")
        .fetch_one(&pool).await.unwrap_or(0);

    Ok(Json(ApiResponse::success(format!(
        "Refreshed: {} teams, {} matches", team_count, match_count
    ))))
}

// POST /predictions/generate - Generate predictions for upcoming matches
async fn generate_predictions_handler(
    State(pool): State<SqlitePool>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let prediction_engine = PredictionEngine::new();
    
    match get_upcoming_matches(&pool, None).await {
        Ok(matches) => {
            match prediction_engine.generate_predictions(&pool, &matches).await {
                Ok(()) => Ok(Json(ApiResponse::success(format!("Generated predictions for {} matches", matches.len())))),
                Err(e) => {
                    tracing::error!("Failed to generate predictions: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to fetch matches for prediction: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Helper function to generate custom datasets
async fn generate_custom_dataset(
    pool: &SqlitePool,
    request: DatasetRequest,
) -> anyhow::Result<DatasetResponse> {
    let mut query = String::from("SELECT ");
    
    // Build dynamic query based on requested stats categories
    let mut columns = vec!["m.id", "m.home_team_name", "m.away_team_name", "m.match_date"];
    
    for category in &request.stats_categories {
        match category.as_str() {
            "basic" => {
                columns.extend(&["m.home_score", "m.away_score", "m.status"]);
            }
            "teams" => {
                columns.extend(&["ht.elo_rating as home_elo", "at.elo_rating as away_elo"]);
            }
            "predictions" => {
                columns.extend(&["p.home_win_probability", "p.away_win_probability", "p.draw_probability"]);
            }
            _ => {}
        }
    }
    
    query.push_str(&columns.join(", "));
    query.push_str(" FROM matches m ");
    
    if request.stats_categories.contains(&"teams".to_string()) {
        query.push_str("LEFT JOIN teams ht ON m.home_team_id = ht.id ");
        query.push_str("LEFT JOIN teams at ON m.away_team_id = at.id ");
    }
    
    if request.stats_categories.contains(&"predictions".to_string()) {
        query.push_str("LEFT JOIN predictions p ON m.id = p.match_id ");
    }
    
    query.push_str("WHERE 1=1 ");
    
    // Add filters
    if !request.sport.is_empty() {
        query.push_str(&format!("AND m.sport = '{}' ", request.sport));
    }
    
    if let Some(date_from) = request.date_from {
        query.push_str(&format!("AND m.match_date >= '{}' ", date_from.to_rfc3339()));
    }
    
    if let Some(date_to) = request.date_to {
        query.push_str(&format!("AND m.match_date <= '{}' ", date_to.to_rfc3339()));
    }
    
    query.push_str("ORDER BY m.match_date DESC LIMIT 1000");
    
    let rows = sqlx::query(&query).fetch_all(pool).await?;
    
    // Generate file based on format
    let filename = format!("dataset_{}_{}.{}", 
        request.sport, 
        chrono::Utc::now().timestamp(), 
        request.format
    );
    
    let file_path = format!("../data/exports/{}", filename);
    
    // Create exports directory if it doesn't exist
    tokio::fs::create_dir_all("../data/exports").await?;
    
    // Strip SQL aliases from header names:
    //   "m.home_team_name"        → "home_team_name"
    //   "ht.elo_rating as home_elo" → "home_elo"
    let headers: Vec<String> = columns.iter().map(|col| {
        // Take the alias after " as " if present, otherwise use the raw column expression.
        let col = if let Some(pos) = col.to_lowercase().find(" as ") {
            col[pos + 4..].trim()
        } else {
            col.trim()
        };
        // Strip the "table." prefix from "table.column".
        if let Some(dot) = col.rfind('.') { col[dot + 1..].to_string() } else { col.to_string() }
    }).collect();

    // Helper: read a row cell as a plain string regardless of its SQLite type.
    let cell_to_string = |row: &sqlx::sqlite::SqliteRow, i: usize| -> String {
        if let Ok(v) = row.try_get::<String, _>(i)  { return v; }
        if let Ok(v) = row.try_get::<f64, _>(i)     { return v.to_string(); }
        if let Ok(v) = row.try_get::<i64, _>(i)     { return v.to_string(); }
        if let Ok(v) = row.try_get::<bool, _>(i)    { return v.to_string(); }
        String::new() // NULL
    };

    match request.format.as_str() {
        "csv" => {
            let mut writer = csv::Writer::from_path(&file_path)?;
            writer.write_record(&headers)?;
            for row in &rows {
                let record: Vec<String> = (0..columns.len())
                    .map(|i| cell_to_string(row, i))
                    .collect();
                writer.write_record(&record)?;
            }
            writer.flush()?;
        }
        "json" => {
            let data: Vec<HashMap<String, String>> = rows.iter().map(|row| {
                headers.iter().enumerate()
                    .map(|(i, h)| (h.clone(), cell_to_string(row, i)))
                    .collect()
            }).collect();
            let json_str = serde_json::to_string_pretty(&data)?;
            tokio::fs::write(&file_path, json_str).await?;
        }
        _ => return Err(anyhow::anyhow!("Unsupported format: {}", request.format)),
    }
    
    Ok(DatasetResponse {
        download_url: format!("/downloads/{}", filename),
        format: request.format,
        rows: rows.len(),
        generated_at: chrono::Utc::now(),
    })
}