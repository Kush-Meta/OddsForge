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

/// Ingests games_details.csv → game_box_stats table.
/// Aggregates all player rows per (GAME_ID, TEAM_ID) into team-level box scores,
/// then stores one row per team per game with the game date (from games.csv).
pub async fn ingest_box_stats(pool: &SqlitePool, data_dir: &str) -> Result<usize> {
    let details_path = format!("{}/games_details.csv", data_dir);
    let games_path   = format!("{}/games.csv", data_dir);
    let teams_path   = format!("{}/teams.csv", data_dir);

    if !std::path::Path::new(&details_path).exists() {
        anyhow::bail!("games_details.csv not found at {}", details_path);
    }

    // ── Step 1: build kaggle GAME_ID → date ─────────────────────────────────
    #[derive(Debug, Deserialize)]
    struct GDateRow {
        #[serde(rename = "GAME_DATE_EST")]
        game_date: String,
        #[serde(rename = "GAME_ID")]
        game_id: String,
    }
    let mut game_dates: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if std::path::Path::new(&games_path).exists() {
        let mut rdr = Reader::from_path(&games_path)?;
        for row in rdr.deserialize::<GDateRow>().flatten() {
            game_dates.insert(row.game_id, row.game_date[..10].to_string());
        }
    }

    // ── Step 2: build kaggle TEAM_ID → our team_id (same logic as games ingest) ──
    let mut kaggle_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    if std::path::Path::new(&teams_path).exists() {
        let mut rdr = Reader::from_path(&teams_path)?;
        for row in rdr.deserialize::<TeamRow>().flatten() {
            kaggle_names.insert(row.team_id, format!("{} {}", row.city, row.nickname));
        }
    }
    let our_rows = sqlx::query("SELECT id, name FROM teams WHERE sport = 'basketball'")
        .fetch_all(pool).await?;
    let our_teams: Vec<(String, String)> = our_rows.iter()
        .map(|r| (r.get::<String, _>("id"), r.get::<String, _>("name"))).collect();

    let find_team = |kaggle_tid: i64| -> Option<String> {
        let name = kaggle_names.get(&kaggle_tid).map(|s| s.as_str()).unwrap_or("");
        if name.is_empty() { return None; }
        let kn = name.to_lowercase();
        let kn_nick = kn.split_whitespace().last().unwrap_or("");
        for (id, our_name) in &our_teams {
            let on = our_name.to_lowercase();
            if on.split_whitespace().last().unwrap_or("") == kn_nick { return Some(id.clone()); }
        }
        our_teams.iter()
            .map(|(id, on)| (id, jaro_winkler(name, on)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .filter(|(_, s)| *s >= 0.82)
            .map(|(id, _)| id.clone())
    };

    // ── Step 3: aggregate players → team-game box scores ────────────────────
    #[derive(Debug, Deserialize)]
    struct DetailRow {
        #[serde(rename = "GAME_ID")]   game_id: String,
        #[serde(rename = "TEAM_ID")]   team_id: i64,
        #[serde(rename = "PTS")]       pts: Option<f64>,
        #[serde(rename = "FGM")]       fgm: Option<f64>,
        #[serde(rename = "FGA")]       fga: Option<f64>,
        #[serde(rename = "FG3M")]      fg3m: Option<f64>,
        #[serde(rename = "FG3A")]      fg3a: Option<f64>,
        #[serde(rename = "FTM")]       ftm: Option<f64>,
        #[serde(rename = "FTA")]       fta: Option<f64>,
        #[serde(rename = "OREB")]      oreb: Option<f64>,
        #[serde(rename = "DREB")]      dreb: Option<f64>,
        #[serde(rename = "TO")]        tov: Option<f64>,
    }

    // (game_id, kaggle_team_id) → accumulated stats
    type Key = (String, i64);
    #[derive(Default)]
    struct Acc { pts: f64, fgm: f64, fga: f64, fg3m: f64, fg3a: f64, ftm: f64, fta: f64, oreb: f64, dreb: f64, tov: f64 }
    let mut acc_map: std::collections::HashMap<Key, Acc> = std::collections::HashMap::new();

    let mut rdr = Reader::from_path(&details_path)?;
    let mut total_rows = 0usize;
    for row in rdr.deserialize::<DetailRow>().flatten() {
        let e = acc_map.entry((row.game_id, row.team_id)).or_default();
        e.pts  += row.pts.unwrap_or(0.0);
        e.fgm  += row.fgm.unwrap_or(0.0);
        e.fga  += row.fga.unwrap_or(0.0);
        e.fg3m += row.fg3m.unwrap_or(0.0);
        e.fg3a += row.fg3a.unwrap_or(0.0);
        e.ftm  += row.ftm.unwrap_or(0.0);
        e.fta  += row.fta.unwrap_or(0.0);
        e.oreb += row.oreb.unwrap_or(0.0);
        e.dreb += row.dreb.unwrap_or(0.0);
        e.tov  += row.tov.unwrap_or(0.0);
        total_rows += 1;
    }
    tracing::info!("Aggregated {} player rows into {} team-game entries", total_rows, acc_map.len());

    // ── Step 4: upsert into game_box_stats ───────────────────────────────────
    let mut inserted = 0usize;
    let mut tid_cache: std::collections::HashMap<i64, Option<String>> = std::collections::HashMap::new();

    for ((game_id, kaggle_tid), stats) in &acc_map {
        let game_date = match game_dates.get(game_id) {
            Some(d) => d.clone(),
            None => continue, // no date → skip
        };
        let our_tid = tid_cache.entry(*kaggle_tid)
            .or_insert_with(|| find_team(*kaggle_tid)).clone();
        let our_tid = match our_tid {
            Some(t) => t,
            None => continue,
        };

        // Deterministic ID: team_id + game_date + game_id
        let id = format!("gbs_{}_{}", our_tid, game_id);

        sqlx::query(
            r#"INSERT OR IGNORE INTO game_box_stats
               (id, team_id, game_date, pts, fgm, fga, fg3m, fg3a, ftm, fta, oreb, dreb, tov)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
        )
        .bind(&id).bind(&our_tid).bind(&game_date)
        .bind(stats.pts).bind(stats.fgm).bind(stats.fga)
        .bind(stats.fg3m).bind(stats.fg3a)
        .bind(stats.ftm).bind(stats.fta)
        .bind(stats.oreb).bind(stats.dreb).bind(stats.tov)
        .execute(pool).await?;

        inserted += 1;
    }

    tracing::info!("game_box_stats: {} rows inserted", inserted);
    Ok(inserted)
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
