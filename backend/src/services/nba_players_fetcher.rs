/// Fetches NBA player rosters and season averages from balldontlie.io v1 API.
///
/// For each NBA team in the DB we:
///   1. Pull the current active roster (≤ 15 players).
///   2. Batch-fetch season averages for all those player IDs.
///   3. Upsert everything into `nba_player_stats`.
///
/// Respects a 24-hour cache window to avoid burning the free-tier quota.
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::env;

use crate::db::{get_teams_by_league, upsert_nba_player_stats};
use crate::models::NbaPlayerStats;

const CURRENT_SEASON: &str = "2025";   // balldontlie year tag for the 2025-26 season
const REFRESH_HOURS: i64 = 24;

// ── balldontlie response shapes ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BdlPlayersResponse {
    data: Vec<BdlPlayer>,
    meta: Option<BdlMeta>,
}

#[derive(Debug, Deserialize)]
struct BdlMeta {
    next_cursor: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BdlPlayer {
    id: u64,
    first_name: String,
    last_name: String,
    position: Option<String>,
    jersey_number: Option<String>,
    team: Option<BdlPlayerTeam>,
}

#[derive(Debug, Deserialize)]
struct BdlPlayerTeam {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct BdlAvgsResponse {
    data: Vec<BdlAvg>,
}

#[derive(Debug, Deserialize)]
struct BdlAvg {
    player_id: u64,
    games_played: Option<i32>,
    min: Option<String>,
    pts: Option<f64>,
    reb: Option<f64>,
    ast: Option<f64>,
    stl: Option<f64>,
    blk: Option<f64>,
    fg_pct: Option<f64>,
    fg3_pct: Option<f64>,
    turnover: Option<f64>,
}

// ── NbaPlayersFetcher ─────────────────────────────────────────────────────────

pub struct NbaPlayersFetcher {
    client: Client,
    api_key: String,
}

impl NbaPlayersFetcher {
    pub fn new() -> Result<Self> {
        let api_key = env::var("BALLDONTLIE_API_KEY")
            .map_err(|_| anyhow!("BALLDONTLIE_API_KEY not set"))?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            api_key,
        })
    }

    /// Returns true when the player data is older than REFRESH_HOURS.
    pub async fn should_refresh(pool: &SqlitePool) -> bool {
        let fetched_at: Option<String> = sqlx::query_scalar(
            "SELECT fetched_at FROM nba_player_stats LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        match fetched_at {
            None => true,
            Some(ts) => {
                let parsed = chrono::DateTime::parse_from_rfc3339(&ts)
                    .map(|d| d.with_timezone(&Utc));
                match parsed {
                    Ok(t) => Utc::now().signed_duration_since(t).num_hours() >= REFRESH_HOURS,
                    Err(_) => true,
                }
            }
        }
    }

    pub async fn fetch_and_store(&self, pool: &SqlitePool) -> Result<()> {
        let teams = get_teams_by_league(pool, "basketball", "NBA").await?;
        if teams.is_empty() {
            tracing::warn!("No NBA teams in DB — skipping player fetch");
            return Ok(());
        }

        tracing::info!("Fetching NBA player rosters for {} teams…", teams.len());

        let now = Utc::now().to_rfc3339();
        let mut total_stored = 0usize;

        for team in &teams {
            // Extract the numeric balldontlie team ID from our "nba_<id>" format
            let bdl_team_id = match team.id.strip_prefix("nba_") {
                Some(id) => id,
                None => continue,
            };

            // Fetch roster for this team
            let players = match self.fetch_team_roster(bdl_team_id).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("Failed to fetch roster for {} ({}): {}", team.name, team.id, e);
                    continue;
                }
            };

            if players.is_empty() {
                continue;
            }

            // Collect player IDs for the averages batch call
            let player_ids: Vec<u64> = players.iter().map(|p| p.id).collect();

            // Fetch season averages in one call (balldontlie supports up to 100 IDs)
            let avgs = match self.fetch_season_averages(&player_ids).await {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!("Failed to fetch averages for {} players: {}", player_ids.len(), e);
                    Vec::new()
                }
            };

            // Build a lookup map: player_id → BdlAvg
            let avg_map: std::collections::HashMap<u64, &BdlAvg> =
                avgs.iter().map(|a| (a.player_id, a)).collect();

            for player in &players {
                let avg = avg_map.get(&player.id);

                let stats = NbaPlayerStats {
                    player_id:    player.id as i64,
                    team_id:      team.id.clone(),
                    first_name:   player.first_name.clone(),
                    last_name:    player.last_name.clone(),
                    position:     player.position.clone().unwrap_or_default(),
                    jersey_number: player.jersey_number.clone(),
                    pts:          avg.and_then(|a| a.pts).unwrap_or(0.0),
                    reb:          avg.and_then(|a| a.reb).unwrap_or(0.0),
                    ast:          avg.and_then(|a| a.ast).unwrap_or(0.0),
                    stl:          avg.and_then(|a| a.stl).unwrap_or(0.0),
                    blk:          avg.and_then(|a| a.blk).unwrap_or(0.0),
                    fg_pct:       avg.and_then(|a| a.fg_pct).unwrap_or(0.0),
                    fg3_pct:      avg.and_then(|a| a.fg3_pct).unwrap_or(0.0),
                    min:          avg.and_then(|a| a.min.clone()).unwrap_or_else(|| "0".to_string()),
                    games_played: avg.and_then(|a| a.games_played).unwrap_or(0),
                    season:       CURRENT_SEASON.to_string(),
                    fetched_at:   now.clone(),
                };

                if let Err(e) = upsert_nba_player_stats(pool, &stats).await {
                    tracing::warn!("Failed to upsert player {}: {}", player.id, e);
                } else {
                    total_stored += 1;
                }
            }

            // Small pause between teams to be kind to the free tier
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        tracing::info!("NBA player stats stored: {} players", total_stored);
        Ok(())
    }

    /// Fetch the active roster for a single team (balldontlie numeric ID).
    async fn fetch_team_roster(&self, bdl_team_id: &str) -> Result<Vec<BdlPlayer>> {
        let url = format!(
            "https://api.balldontlie.io/v1/players?team_ids[]={}&per_page=25",
            bdl_team_id
        );

        let resp = self.client
            .get(&url)
            .header("Authorization", &self.api_key)
            .send().await?;

        if resp.status() == 429 {
            return Err(anyhow!("Rate limited by balldontlie.io"));
        }

        if !resp.status().is_success() {
            return Err(anyhow!("Players API returned {}", resp.status()));
        }

        let data: BdlPlayersResponse = resp.json().await?;
        Ok(data.data)
    }

    /// Fetch season averages for a batch of player IDs.
    async fn fetch_season_averages(&self, player_ids: &[u64]) -> Result<Vec<BdlAvg>> {
        if player_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build query string: player_ids[]=1&player_ids[]=2&...
        let id_params: String = player_ids
            .iter()
            .map(|id| format!("player_ids[]={}", id))
            .collect::<Vec<_>>()
            .join("&");

        let url = format!(
            "https://api.balldontlie.io/v1/season_averages?season={}&{}",
            CURRENT_SEASON, id_params
        );

        let resp = self.client
            .get(&url)
            .header("Authorization", &self.api_key)
            .send().await?;

        if resp.status() == 429 {
            return Err(anyhow!("Rate limited by balldontlie.io"));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Season averages API returned {}: {}", status, body));
        }

        let data: BdlAvgsResponse = resp.json().await?;
        Ok(data.data)
    }
}
