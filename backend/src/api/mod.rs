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
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::db::{create_pool, get_team_by_id, get_teams_by_league, get_upcoming_matches, get_prediction_by_match_id};
use crate::models::{ApiResponse, DatasetRequest, UpcomingMatchWithPrediction, TeamProfile, Team};
use crate::services::{DataFetcher, PredictionEngine};

pub async fn serve(port: u16) -> anyhow::Result<()> {
    let pool = create_pool().await?;
    
    let app = create_router().with_state(pool);
    
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("OddsForge API server listening on port {}", port);
    
    axum::serve(listener, app).await?;
    Ok(())
}

fn create_router() -> Router<SqlitePool> {
    Router::new()
        .route("/health", get(health_check))
        .route("/matches/upcoming", get(get_upcoming_matches_handler))
        .route("/teams/:id/stats", get(get_team_stats_handler))
        .route("/teams/league/:sport/:league", get(get_teams_by_league_handler))
        .route("/predictions/edges", get(get_prediction_edges_handler))
        .route("/datasets/generate", post(generate_dataset_handler))
        .route("/data/fetch", post(fetch_data_handler))
        .route("/predictions/generate", post(generate_predictions_handler))
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

// GET /teams/:id/stats - Get team analytics
async fn get_team_stats_handler(
    State(pool): State<SqlitePool>,
    Path(team_id): Path<String>,
) -> Result<Json<ApiResponse<TeamProfile>>, StatusCode> {
    match get_team_by_id(&pool, &team_id).await {
        Ok(Some(team)) => {
            // TODO: Implement full team profile with stats, recent matches, ELO history
            let profile = TeamProfile {
                team,
                current_stats: crate::models::TeamStats {
                    id: uuid::Uuid::new_v4().to_string(),
                    team_id: team_id.clone(),
                    season: "2024-25".to_string(),
                    matches_played: 0,
                    wins: 0,
                    draws: Some(0),
                    losses: 0,
                    goals_for: Some(0),
                    goals_against: Some(0),
                    points_for: Some(0),
                    points_against: Some(0),
                    form: "".to_string(),
                    updated_at: chrono::Utc::now(),
                },
                recent_matches: vec![],
                elo_history: vec![],
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
    
    let result = match request.sport.as_deref() {
        Some("football") => {
            tokio::try_join!(
                fetcher.fetch_epl_teams(&pool),
                fetcher.fetch_champions_league_teams(&pool),
                fetcher.fetch_epl_matches(&pool)
            ).map(|_| "Football data fetched successfully")
        }
        Some("basketball") => {
            tokio::try_join!(
                fetcher.fetch_nba_teams(&pool),
                fetcher.fetch_nba_games(&pool)
            ).map(|_| "Basketball data fetched successfully")
        }
        _ => {
            fetcher.fetch_all_data(&pool).await.map(|_| "All sports data fetched successfully")
        }
    };
    
    match result {
        Ok(message) => Ok(Json(ApiResponse::success(message.to_string()))),
        Err(e) => {
            tracing::error!("Failed to fetch data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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
    
    match request.format.as_str() {
        "csv" => {
            let mut writer = csv::Writer::from_path(&file_path)?;
            
            // Write headers
            writer.write_record(&columns)?;
            
            // Write data rows
            for row in &rows {
                let mut record = Vec::new();
                for (i, _col) in columns.iter().enumerate() {
                    // This is simplified - in practice you'd need proper type handling
                    record.push(format!("{:?}", row.try_get::<String, _>(i).unwrap_or_default()));
                }
                writer.write_record(&record)?;
            }
            
            writer.flush()?;
        }
        "json" => {
            let mut data = Vec::new();
            for row in &rows {
                let mut record = HashMap::new();
                for (i, col) in columns.iter().enumerate() {
                    record.insert(col.to_string(), row.try_get::<String, _>(i).unwrap_or_default());
                }
                data.push(record);
            }
            
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