//! Kaggle NBA Games Dataset Ingestion
//!
//! Expects Wyatt Walsh's "NBA Games" dataset:
//!   <dir>/games.csv         — one row per game, includes scores
//!   <dir>/teams.csv         — team ID → name mapping (optional)
//!
//! games.csv columns used:
//!   GAME_DATE_EST, HOME_TEAM_ID, VISITOR_TEAM_ID, PTS_home, PTS_away

use anyhow::{Context, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use csv::Reader;
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use strsim::jaro_winkler;

#[derive(Debug, Deserialize)]
struct GameRow {
    #[serde(rename = "GAME_DATE_EST")]
    game_date: String,
    #[serde(rename = "HOME_TEAM_ID")]
    home_team_id: i64,
    #[serde(rename = "VISITOR_TEAM_ID")]
    visitor_team_id: i64,
    #[serde(rename = "PTS_home")]
    pts_home: Option<f64>,
    #[serde(rename = "PTS_away")]
    pts_away: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TeamRow {
    #[serde(rename = "TEAM_ID")]
    team_id: i64,
    #[serde(rename = "NICKNAME")]
    nickname: String,
    #[serde(rename = "CITY")]
    city: String,
}

pub async fn ingest_kaggle_games(pool: &SqlitePool, data_dir: &str) -> Result<usize> {
    let games_path = format!("{}/games.csv", data_dir);
    let teams_path = format!("{}/teams.csv", data_dir);

    if !std::path::Path::new(&games_path).exists() {
        anyhow::bail!("games.csv not found at {}. Download the Kaggle NBA Games dataset first.", games_path);
    }

    // Load Kaggle team ID → name
    let mut kaggle_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    if std::path::Path::new(&teams_path).exists() {
        let mut rdr = Reader::from_path(&teams_path).context("Failed to open teams.csv")?;
        for result in rdr.deserialize::<TeamRow>() {
            if let Ok(row) = result {
                kaggle_names.insert(row.team_id, format!("{} {}", row.city, row.nickname));
            }
        }
        tracing::info!("Loaded {} Kaggle team names", kaggle_names.len());
    }

    // Load our NBA teams for fuzzy matching
    let our_rows = sqlx::query("SELECT id, name FROM teams WHERE sport = 'basketball'")
        .fetch_all(pool).await?;
    let our_teams: Vec<(String, String)> = our_rows.iter()
        .map(|r| (r.get::<String, _>("id"), r.get::<String, _>("name")))
        .collect();

    // Fuzzy-match kaggle name → our team ID
    let find_team = |name: &str| -> Option<String> {
        if name.is_empty() { return None; }
        // Try nickname (last word) exact match first
        let kn = name.to_lowercase();
        let kn_nick = kn.split_whitespace().last().unwrap_or("");
        for (id, our_name) in &our_teams {
            let on_nick = our_name.to_lowercase();
            let on_nick = on_nick.split_whitespace().last().unwrap_or("");
            if kn_nick == on_nick { return Some(id.clone()); }
        }
        // Jaro-Winkler fallback
        let best = our_teams.iter()
            .map(|(id, oname)| (id, jaro_winkler(name, oname)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((id, score)) = best {
            if score >= 0.82 { return Some(id.clone()); }
        }
        None
    };

    let mut id_cache: std::collections::HashMap<i64, Option<String>> = std::collections::HashMap::new();
    let mut rdr = Reader::from_path(&games_path).context("Failed to open games.csv")?;
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let now = Utc::now().to_rfc3339();

    for result in rdr.deserialize::<GameRow>() {
        let row = match result {
            Ok(r) => r,
            Err(e) => { tracing::debug!("CSV parse error: {}", e); skipped += 1; continue; }
        };

        let (pts_h, pts_a) = match (row.pts_home, row.pts_away) {
            (Some(h), Some(a)) if h > 0.0 || a > 0.0 => (h as i32, a as i32),
            _ => { skipped += 1; continue; }
        };

        let home_our_id = id_cache.entry(row.home_team_id).or_insert_with(|| {
            let name = kaggle_names.get(&row.home_team_id).map(|s| s.as_str()).unwrap_or("");
            find_team(name)
        }).clone();
        let away_our_id = id_cache.entry(row.visitor_team_id).or_insert_with(|| {
            let name = kaggle_names.get(&row.visitor_team_id).map(|s| s.as_str()).unwrap_or("");
            find_team(name)
        }).clone();

        let (home_id, away_id) = match (home_our_id, away_our_id) {
            (Some(h), Some(a)) => (h, a),
            _ => { skipped += 1; continue; }
        };

        let naive = NaiveDate::parse_from_str(&row.game_date, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(&row.game_date, "%m/%d/%Y"))
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2020, 1, 1).unwrap());
        let match_date = Utc.from_utc_datetime(&naive.and_hms_opt(19, 0, 0).unwrap()).to_rfc3339();

        let home_name: String = sqlx::query("SELECT name FROM teams WHERE id = ?")
            .bind(&home_id).fetch_optional(pool).await?
            .map(|r| r.get("name")).unwrap_or_else(|| home_id.clone());
        let away_name: String = sqlx::query("SELECT name FROM teams WHERE id = ?")
            .bind(&away_id).fetch_optional(pool).await?
            .map(|r| r.get("name")).unwrap_or_else(|| away_id.clone());

        // Stable deterministic ID: date + team IDs
        let match_id = format!("kgl_{}_{}_{}", naive.format("%Y%m%d"), row.home_team_id, row.visitor_team_id);

        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM matches WHERE id = ?")
            .bind(&match_id).fetch_one(pool).await.unwrap_or(0);
        if exists > 0 { skipped += 1; continue; }

        sqlx::query(
            r#"INSERT OR IGNORE INTO matches
               (id, home_team_id, away_team_id, home_team_name, away_team_name,
                sport, league, match_date, status, home_score, away_score, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, 'basketball', 'NBA', ?, 'finished', ?, ?, ?, ?)"#
        )
        .bind(&match_id)
        .bind(&home_id).bind(&away_id)
        .bind(&home_name).bind(&away_name)
        .bind(&match_date)
        .bind(pts_h).bind(pts_a)
        .bind(&now).bind(&now)
        .execute(pool).await?;

        inserted += 1;
        if inserted % 1000 == 0 {
            tracing::info!("Kaggle ingest: {} games inserted...", inserted);
        }
    }

    tracing::info!("Kaggle ingest complete: {} inserted, {} skipped", inserted, skipped);
    Ok(inserted)
}
