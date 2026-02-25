pub mod seed;
pub use seed::seed_data;

pub async fn clear_all_data(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM predictions").execute(pool).await?;
    sqlx::query("DELETE FROM matches").execute(pool).await?;
    sqlx::query("DELETE FROM season_stats").execute(pool).await?;
    sqlx::query("DELETE FROM elo_history").execute(pool).await?;
    sqlx::query("DELETE FROM teams").execute(pool).await?;
    tracing::info!("All data cleared");
    Ok(())
}

use anyhow::Result;
use chrono::Utc;
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};
use std::env;
use std::str::FromStr;

use crate::models::*;

pub async fn create_pool() -> Result<SqlitePool> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:../data/oddsforge.db".to_string());

    // Strip the "sqlite:" prefix to get the file path, create parent dir if needed
    let file_path = database_url
        .strip_prefix("sqlite:///")
        .or_else(|| database_url.strip_prefix("sqlite://"))
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(&database_url);

    if let Some(parent) = std::path::Path::new(file_path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
    }

    let options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(options).await?;
    Ok(pool)
}

/// Called from the CLI where no pool exists yet.
pub async fn init_database() -> Result<()> {
    let pool = create_pool().await?;
    init_database_with_pool(&pool).await
}

/// Called from the server so schema creation shares the main pool.
pub async fn init_database_with_pool(pool: &SqlitePool) -> Result<()> {
    let pool = pool.clone(); // clone is cheap (Arc refcount) — gives us SqlitePool, not &SqlitePool
    
    // Create tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS teams (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            sport TEXT NOT NULL,
            league TEXT NOT NULL,
            logo_url TEXT,
            elo_rating REAL NOT NULL DEFAULT 1200.0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS matches (
            id TEXT PRIMARY KEY,
            home_team_id TEXT NOT NULL,
            away_team_id TEXT NOT NULL,
            home_team_name TEXT NOT NULL,
            away_team_name TEXT NOT NULL,
            sport TEXT NOT NULL,
            league TEXT NOT NULL,
            match_date TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'scheduled',
            home_score INTEGER,
            away_score INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (home_team_id) REFERENCES teams (id),
            FOREIGN KEY (away_team_id) REFERENCES teams (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS predictions (
            id TEXT PRIMARY KEY,
            match_id TEXT NOT NULL,
            home_win_probability REAL NOT NULL,
            away_win_probability REAL NOT NULL,
            draw_probability REAL,
            model_version TEXT NOT NULL,
            confidence_score REAL NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (match_id) REFERENCES matches (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS team_stats (
            id TEXT PRIMARY KEY,
            team_id TEXT NOT NULL,
            season TEXT NOT NULL,
            matches_played INTEGER NOT NULL DEFAULT 0,
            wins INTEGER NOT NULL DEFAULT 0,
            draws INTEGER,
            losses INTEGER NOT NULL DEFAULT 0,
            goals_for INTEGER,
            goals_against INTEGER,
            points_for INTEGER,
            points_against INTEGER,
            form TEXT,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (team_id) REFERENCES teams (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS elo_history (
            id TEXT PRIMARY KEY,
            team_id TEXT NOT NULL,
            date TEXT NOT NULL,
            elo_rating REAL NOT NULL,
            match_id TEXT,
            FOREIGN KEY (team_id) REFERENCES teams (id),
            FOREIGN KEY (match_id) REFERENCES matches (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Create indexes
    // market_odds: one row per match, best available odds from The Odds API
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS market_odds (
            match_id    TEXT PRIMARY KEY,
            bookmaker   TEXT NOT NULL,
            home_odds   REAL NOT NULL,
            draw_odds   REAL,
            away_odds   REAL NOT NULL,
            fetched_at  TEXT NOT NULL,
            FOREIGN KEY (match_id) REFERENCES matches (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // odds_fetch_log: tracks last successful API call per sport_key to avoid burning quota
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS odds_fetch_log (
            sport_key    TEXT PRIMARY KEY,
            last_fetched TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_matches_date ON matches(match_date)")
        .execute(&pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_matches_status ON matches(status)")
        .execute(&pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_teams_sport_league ON teams(sport, league)")
        .execute(&pool)
        .await?;

    tracing::info!("Database initialized successfully");
    Ok(())
}

// Team operations
pub async fn insert_team(pool: &SqlitePool, team: &Team) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO teams 
        (id, name, sport, league, logo_url, elo_rating, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&team.id)
    .bind(&team.name)
    .bind(&team.sport)
    .bind(&team.league)
    .bind(&team.logo_url)
    .bind(team.elo_rating)
    .bind(team.created_at.to_rfc3339())
    .bind(team.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    
    Ok(())
}

pub async fn get_team_by_id(pool: &SqlitePool, team_id: &str) -> Result<Option<Team>> {
    let row = sqlx::query("SELECT * FROM teams WHERE id = ?")
        .bind(team_id)
        .fetch_optional(pool)
        .await?;
    
    if let Some(row) = row {
        Ok(Some(Team {
            id: row.get("id"),
            name: row.get("name"),
            sport: row.get("sport"),
            league: row.get("league"),
            logo_url: row.get("logo_url"),
            elo_rating: row.get("elo_rating"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
        }))
    } else {
        Ok(None)
    }
}

pub async fn get_teams_by_league(pool: &SqlitePool, sport: &str, league: &str) -> Result<Vec<Team>> {
    let rows = sqlx::query("SELECT * FROM teams WHERE sport = ? AND league = ? ORDER BY name")
        .bind(sport)
        .bind(league)
        .fetch_all(pool)
        .await?;
    
    let mut teams = Vec::new();
    for row in rows {
        teams.push(Team {
            id: row.get("id"),
            name: row.get("name"),
            sport: row.get("sport"),
            league: row.get("league"),
            logo_url: row.get("logo_url"),
            elo_rating: row.get("elo_rating"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
        });
    }
    
    Ok(teams)
}

// Match operations
pub async fn insert_match(pool: &SqlitePool, match_data: &Match) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO matches 
        (id, home_team_id, away_team_id, home_team_name, away_team_name, sport, league, 
         match_date, status, home_score, away_score, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&match_data.id)
    .bind(&match_data.home_team_id)
    .bind(&match_data.away_team_id)
    .bind(&match_data.home_team_name)
    .bind(&match_data.away_team_name)
    .bind(&match_data.sport)
    .bind(&match_data.league)
    .bind(match_data.match_date.to_rfc3339())
    .bind(&match_data.status)
    .bind(match_data.home_score)
    .bind(match_data.away_score)
    .bind(match_data.created_at.to_rfc3339())
    .bind(match_data.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    
    Ok(())
}

pub async fn get_upcoming_matches(pool: &SqlitePool, sport: Option<&str>) -> Result<Vec<Match>> {
    let query = if let Some(sport) = sport {
        "SELECT * FROM matches WHERE match_date > datetime('now') AND sport = ? ORDER BY match_date LIMIT 50"
    } else {
        "SELECT * FROM matches WHERE match_date > datetime('now') ORDER BY match_date LIMIT 50"
    };
    
    let mut query_builder = sqlx::query(query);
    if let Some(sport) = sport {
        query_builder = query_builder.bind(sport);
    }
    
    let rows = query_builder.fetch_all(pool).await?;
    
    let mut matches = Vec::new();
    for row in rows {
        matches.push(Match {
            id: row.get("id"),
            home_team_id: row.get("home_team_id"),
            away_team_id: row.get("away_team_id"),
            home_team_name: row.get("home_team_name"),
            away_team_name: row.get("away_team_name"),
            sport: row.get("sport"),
            league: row.get("league"),
            match_date: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("match_date"))?.with_timezone(&Utc),
            status: row.get("status"),
            home_score: row.get("home_score"),
            away_score: row.get("away_score"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
        });
    }
    
    Ok(matches)
}

pub async fn get_finished_matches_ordered(pool: &SqlitePool) -> Result<Vec<Match>> {
    let rows = sqlx::query(
        "SELECT * FROM matches WHERE status = 'finished' AND home_score IS NOT NULL ORDER BY match_date ASC"
    )
    .fetch_all(pool)
    .await?;

    let mut matches = Vec::new();
    for row in rows {
        matches.push(Match {
            id:             row.get("id"),
            home_team_id:   row.get("home_team_id"),
            away_team_id:   row.get("away_team_id"),
            home_team_name: row.get("home_team_name"),
            away_team_name: row.get("away_team_name"),
            sport:          row.get("sport"),
            league:         row.get("league"),
            match_date:     chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("match_date"))?.with_timezone(&Utc),
            status:         row.get("status"),
            home_score:     row.get("home_score"),
            away_score:     row.get("away_score"),
            created_at:     chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            updated_at:     chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
        });
    }
    Ok(matches)
}

// Prediction operations
pub async fn insert_prediction(pool: &SqlitePool, prediction: &Prediction) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO predictions 
        (id, match_id, home_win_probability, away_win_probability, draw_probability, 
         model_version, confidence_score, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&prediction.id)
    .bind(&prediction.match_id)
    .bind(prediction.home_win_probability)
    .bind(prediction.away_win_probability)
    .bind(prediction.draw_probability)
    .bind(&prediction.model_version)
    .bind(prediction.confidence_score)
    .bind(prediction.created_at.to_rfc3339())
    .execute(pool)
    .await?;
    
    Ok(())
}

pub async fn get_prediction_by_match_id(pool: &SqlitePool, match_id: &str) -> Result<Option<Prediction>> {
    let row = sqlx::query("SELECT * FROM predictions WHERE match_id = ? ORDER BY created_at DESC LIMIT 1")
        .bind(match_id)
        .fetch_optional(pool)
        .await?;
    
    if let Some(row) = row {
        Ok(Some(Prediction {
            id: row.get("id"),
            match_id: row.get("match_id"),
            home_win_probability: row.get("home_win_probability"),
            away_win_probability: row.get("away_win_probability"),
            draw_probability: row.get("draw_probability"),
            model_version: row.get("model_version"),
            confidence_score: row.get("confidence_score"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
        }))
    } else {
        Ok(None)
    }
}

// Additional query helpers

pub async fn get_all_teams(pool: &SqlitePool) -> Result<Vec<Team>> {
    let rows = sqlx::query("SELECT * FROM teams ORDER BY sport, league, elo_rating DESC")
        .fetch_all(pool)
        .await?;

    let mut teams = Vec::new();
    for row in rows {
        teams.push(Team {
            id: row.get("id"),
            name: row.get("name"),
            sport: row.get("sport"),
            league: row.get("league"),
            logo_url: row.get("logo_url"),
            elo_rating: row.get("elo_rating"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
        });
    }
    Ok(teams)
}

pub async fn get_team_current_stats(pool: &SqlitePool, team_id: &str) -> Result<Option<TeamStats>> {
    let row = sqlx::query(
        "SELECT * FROM team_stats WHERE team_id = ? ORDER BY season DESC LIMIT 1"
    )
    .bind(team_id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        Ok(Some(TeamStats {
            id: row.get("id"),
            team_id: row.get("team_id"),
            season: row.get("season"),
            matches_played: row.get("matches_played"),
            wins: row.get("wins"),
            draws: row.get("draws"),
            losses: row.get("losses"),
            goals_for: row.get("goals_for"),
            goals_against: row.get("goals_against"),
            points_for: row.get("points_for"),
            points_against: row.get("points_against"),
            form: row.get::<Option<String>, _>("form").unwrap_or_default(),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
        }))
    } else {
        Ok(None)
    }
}

pub async fn get_team_recent_matches(pool: &SqlitePool, team_id: &str, limit: i64) -> Result<Vec<Match>> {
    let rows = sqlx::query(
        r#"SELECT * FROM matches
           WHERE (home_team_id = ? OR away_team_id = ?) AND status = 'finished'
           ORDER BY match_date DESC LIMIT ?"#
    )
    .bind(team_id)
    .bind(team_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut matches = Vec::new();
    for row in rows {
        matches.push(Match {
            id: row.get("id"),
            home_team_id: row.get("home_team_id"),
            away_team_id: row.get("away_team_id"),
            home_team_name: row.get("home_team_name"),
            away_team_name: row.get("away_team_name"),
            sport: row.get("sport"),
            league: row.get("league"),
            match_date: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("match_date"))?.with_timezone(&Utc),
            status: row.get("status"),
            home_score: row.get("home_score"),
            away_score: row.get("away_score"),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
        });
    }
    Ok(matches)
}

// Market odds operations

pub async fn upsert_market_odds(
    pool: &SqlitePool,
    match_id: &str,
    bookmaker: &str,
    home_odds: f64,
    draw_odds: Option<f64>,
    away_odds: f64,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"INSERT INTO market_odds (match_id, bookmaker, home_odds, draw_odds, away_odds, fetched_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(match_id) DO UPDATE SET
               bookmaker  = excluded.bookmaker,
               home_odds  = excluded.home_odds,
               draw_odds  = excluded.draw_odds,
               away_odds  = excluded.away_odds,
               fetched_at = excluded.fetched_at"#,
    )
    .bind(match_id)
    .bind(bookmaker)
    .bind(home_odds)
    .bind(draw_odds)
    .bind(away_odds)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_market_odds(pool: &SqlitePool, match_id: &str) -> Result<Option<crate::models::MarketOdds>> {
    let row = sqlx::query(
        "SELECT match_id, bookmaker, home_odds, draw_odds, away_odds, fetched_at FROM market_odds WHERE match_id = ?"
    )
    .bind(match_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| crate::models::MarketOdds {
        match_id:   r.get("match_id"),
        bookmaker:  r.get("bookmaker"),
        home_odds:  r.get("home_odds"),
        draw_odds:  r.get("draw_odds"),
        away_odds:  r.get("away_odds"),
        fetched_at: r.get("fetched_at"),
    }))
}

pub async fn insert_elo_history(
    pool: &SqlitePool,
    team_id: &str,
    date: chrono::DateTime<Utc>,
    elo_rating: f64,
    match_id: &str,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO elo_history (id, team_id, date, elo_rating, match_id) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(id)
    .bind(team_id)
    .bind(date.to_rfc3339())
    .bind(elo_rating)
    .bind(match_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_elo_history(pool: &SqlitePool, team_id: &str) -> Result<Vec<EloHistoryPoint>> {
    let rows = sqlx::query(
        "SELECT * FROM elo_history WHERE team_id = ? ORDER BY date ASC"
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;

    let mut history = Vec::new();
    for row in rows {
        history.push(EloHistoryPoint {
            team_id: row.get("team_id"),
            date: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("date"))?.with_timezone(&Utc),
            elo_rating: row.get("elo_rating"),
            match_id: row.get("match_id"),
        });
    }
    Ok(history)
}