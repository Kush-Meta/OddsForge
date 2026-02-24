pub mod seed;
pub use seed::seed_data;

use anyhow::Result;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use std::env;

use crate::models::*;

pub async fn create_pool() -> Result<SqlitePool> {
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:../data/oddsforge.db".to_string());
    let pool = SqlitePool::connect(&database_url).await?;
    Ok(pool)
}

pub async fn init_database() -> Result<()> {
    let pool = create_pool().await?;
    
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