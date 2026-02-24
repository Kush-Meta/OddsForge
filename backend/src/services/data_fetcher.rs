use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::env;

use crate::db::{insert_match, insert_team};
use crate::models::{Match, Team};

// ── football-data.org structures ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct FootballDataTeams {
    pub teams: Vec<FootballTeam>,
}

#[derive(Debug, Deserialize)]
pub struct FootballTeam {
    pub id: u32,
    pub name: String,
    pub crest: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FootballDataMatches {
    pub matches: Vec<FootballMatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootballMatch {
    pub id: u32,
    pub utc_date: String,
    pub status: String,
    pub home_team: MatchTeam,
    pub away_team: MatchTeam,
    pub score: MatchScore,
}

#[derive(Debug, Deserialize)]
pub struct MatchTeam {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchScore {
    pub full_time: Option<Score>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Score {
    pub home: Option<u32>,
    pub away: Option<u32>,
}

// ── balldontlie.io structures ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NbaMeta {
    pub next_cursor: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct NbaTeamsResponse {
    pub data: Vec<NbaTeam>,
    pub meta: Option<NbaMeta>,
}

#[derive(Debug, Deserialize)]
pub struct NbaTeam {
    pub id: u32,
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
pub struct NbaGamesResponse {
    pub data: Vec<NbaGame>,
    pub meta: Option<NbaMeta>,
}

#[derive(Debug, Deserialize)]
pub struct NbaGame {
    pub id: u32,
    pub date: String,
    pub home_team: NbaTeam,
    pub visitor_team: NbaTeam,
    pub home_team_score: Option<u32>,
    pub visitor_team_score: Option<u32>,
    pub status: String,
}

// ── DataFetcher ──────────────────────────────────────────────────────────────

pub struct DataFetcher {
    client: Client,
    football_api_key: Option<String>,
    nba_api_key: Option<String>,
}

impl DataFetcher {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            football_api_key: env::var("FOOTBALL_DATA_API_KEY").ok(),
            nba_api_key: env::var("BALLDONTLIE_API_KEY").ok(),
        }
    }

    pub fn has_football_key(&self) -> bool { self.football_api_key.is_some() }
    pub fn has_nba_key(&self)      -> bool { self.nba_api_key.is_some() }

    // ── EPL ─────────────────────────────────────────────────────────────────

    pub async fn fetch_epl_teams(&self, pool: &SqlitePool) -> Result<()> {
        let api_key = self.football_api_key.as_ref()
            .ok_or_else(|| anyhow!("FOOTBALL_DATA_API_KEY not set"))?;

        tracing::info!("Fetching EPL teams from football-data.org…");

        let response = self.client
            .get("https://api.football-data.org/v4/competitions/PL/teams")
            .header("X-Auth-Token", api_key)
            .send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("EPL teams API error {}: {}", status, body));
        }

        let data: FootballDataTeams = response.json().await?;
        for t in data.teams {
            insert_team(pool, &Team {
                id:         format!("epl_{}", t.id),
                name:       t.name,
                sport:      "football".to_string(),
                league:     "EPL".to_string(),
                logo_url:   t.crest,
                elo_rating: 1200.0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }).await?;
        }

        tracing::info!("EPL teams stored");
        Ok(())
    }

    /// Fetch all EPL matches for the current season (finished + scheduled).
    pub async fn fetch_epl_matches(&self, pool: &SqlitePool) -> Result<()> {
        let api_key = self.football_api_key.as_ref()
            .ok_or_else(|| anyhow!("FOOTBALL_DATA_API_KEY not set"))?;

        tracing::info!("Fetching EPL matches from football-data.org…");

        let response = self.client
            .get("https://api.football-data.org/v4/competitions/PL/matches")
            .header("X-Auth-Token", api_key)
            .send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("EPL matches API error {}: {}", status, body));
        }

        let data: FootballDataMatches = response.json().await?;
        let mut stored = 0usize;

        for m in data.matches {
            let match_date = match DateTime::parse_from_rfc3339(&m.utc_date) {
                Ok(d) => d.with_timezone(&Utc),
                Err(e) => {
                    tracing::warn!("Bad date '{}': {}", m.utc_date, e);
                    continue;
                }
            };

            let status = match m.status.as_str() {
                "FINISHED"            => "finished",
                "IN_PLAY" | "PAUSED"  => "live",
                _                     => "scheduled",   // SCHEDULED, TIMED, POSTPONED …
            };

            // Only store matches with valid team IDs already in the DB
            let match_obj = Match {
                id:              format!("epl_{}", m.id),
                home_team_id:    format!("epl_{}", m.home_team.id),
                away_team_id:    format!("epl_{}", m.away_team.id),
                home_team_name:  m.home_team.name,
                away_team_name:  m.away_team.name,
                sport:           "football".to_string(),
                league:          "EPL".to_string(),
                match_date,
                status:          status.to_string(),
                home_score:      m.score.full_time.as_ref().and_then(|s| s.home.map(|v| v as i32)),
                away_score:      m.score.full_time.as_ref().and_then(|s| s.away.map(|v| v as i32)),
                created_at:      Utc::now(),
                updated_at:      Utc::now(),
            };

            insert_match(pool, &match_obj).await?;
            stored += 1;
        }

        tracing::info!("Stored {} EPL matches", stored);
        Ok(())
    }

    // ── NBA ──────────────────────────────────────────────────────────────────

    pub async fn fetch_nba_teams(&self, pool: &SqlitePool) -> Result<()> {
        let api_key = self.nba_api_key.as_ref()
            .ok_or_else(|| anyhow!("BALLDONTLIE_API_KEY not set"))?;

        tracing::info!("Fetching NBA teams from balldontlie.io…");

        let response = self.client
            .get("https://api.balldontlie.io/v1/teams?per_page=100")
            .header("Authorization", api_key.as_str())
            .send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("NBA teams API error {}: {}", status, body));
        }

        let data: NbaTeamsResponse = response.json().await?;
        for t in data.data {
            insert_team(pool, &Team {
                id:         format!("nba_{}", t.id),
                name:       t.full_name,
                sport:      "basketball".to_string(),
                league:     "NBA".to_string(),
                logo_url:   None,
                elo_rating: 1200.0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }).await?;
        }

        tracing::info!("NBA teams stored");
        Ok(())
    }

    /// Fetch all NBA games for the 2025-26 season, paginating through all results.
    pub async fn fetch_nba_games(&self, pool: &SqlitePool) -> Result<()> {
        self.fetch_nba_games_since(pool, None).await
    }

    /// Fetch only NBA games from the last `days` days (for incremental background refreshes).
    pub async fn fetch_recent_nba_games(&self, pool: &SqlitePool, days: i64) -> Result<()> {
        let since = chrono::Utc::now() - chrono::Duration::days(days);
        self.fetch_nba_games_since(pool, Some(since)).await
    }

    async fn fetch_nba_games_since(&self, pool: &SqlitePool, since: Option<chrono::DateTime<Utc>>) -> Result<()> {
        let api_key = self.nba_api_key.as_ref()
            .ok_or_else(|| anyhow!("BALLDONTLIE_API_KEY not set"))?;

        let label = since.map_or("full season".to_string(), |d| format!("since {}", d.format("%Y-%m-%d")));
        tracing::info!("Fetching NBA 2025-26 games ({}) from balldontlie.io…", label);

        let mut cursor: Option<u64> = None;
        let mut total = 0usize;
        let mut page = 0u32;

        loop {
            page += 1;
            let mut url = format!(
                "https://api.balldontlie.io/v1/games?seasons[]=2025&per_page=100"
            );
            if let Some(d) = since {
                url.push_str(&format!("&start_date={}", d.format("%Y-%m-%d")));
            }
            if let Some(c) = cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            tracing::info!("NBA games page {}…", page);

            // Retry up to 3 times on 429 with exponential backoff
            let data: NbaGamesResponse = {
                let mut attempts = 0u32;
                loop {
                    attempts += 1;
                    let resp = self.client
                        .get(&url)
                        .header("Authorization", api_key.as_str())
                        .send().await?;

                    if resp.status() == 429 {
                        let wait = 2u64.pow(attempts) * 5; // 10s, 20s, 40s
                        tracing::warn!("NBA 429 rate-limited — waiting {}s (attempt {})", wait, attempts);
                        if attempts >= 3 {
                            return Err(anyhow!("NBA API rate limit exceeded after {} attempts", attempts));
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(wait)).await;
                        continue;
                    }

                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        return Err(anyhow!("NBA games API error {}: {}", status, body));
                    }

                    break resp.json().await?;
                }
            };
            let batch_len = data.data.len();

            for g in data.data {
                // balldontlie dates are "YYYY-MM-DD"
                let date_str = format!("{}T00:00:00Z", g.date.trim_end_matches('Z').trim());
                let match_date = match DateTime::parse_from_rfc3339(&date_str) {
                    Ok(d) => d.with_timezone(&Utc),
                    Err(_) => Utc::now(),
                };

                let finished = g.home_team_score.is_some() && g.visitor_team_score.is_some()
                    && g.home_team_score != Some(0) && g.visitor_team_score != Some(0)
                    || g.status.to_lowercase().contains("final");

                let status = if finished { "finished" } else { "scheduled" };

                let match_obj = Match {
                    id:             format!("nba_{}", g.id),
                    home_team_id:   format!("nba_{}", g.home_team.id),
                    away_team_id:   format!("nba_{}", g.visitor_team.id),
                    home_team_name: g.home_team.full_name,
                    away_team_name: g.visitor_team.full_name,
                    sport:          "basketball".to_string(),
                    league:         "NBA".to_string(),
                    match_date,
                    status:         status.to_string(),
                    home_score:     if finished { g.home_team_score.map(|s| s as i32) } else { None },
                    away_score:     if finished { g.visitor_team_score.map(|s| s as i32) } else { None },
                    created_at:     Utc::now(),
                    updated_at:     Utc::now(),
                };

                insert_match(pool, &match_obj).await?;
                total += 1;
            }

            // Advance cursor — stop when next_cursor is None or batch was empty
            cursor = data.meta.and_then(|m| m.next_cursor);
            if cursor.is_none() || batch_len == 0 {
                break;
            }

            // 2 s between pages → max 30 req/min (free tier limit)
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

        tracing::info!("Stored {} NBA games across {} pages", total, page);
        Ok(())
    }

    // ── Combined fetch ───────────────────────────────────────────────────────

    pub async fn fetch_all_data(&self, pool: &SqlitePool) -> Result<()> {
        if self.has_football_key() {
            self.fetch_epl_teams(pool).await?;
            // football-data.org free tier: 10 req/min — wait between calls
            tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
            self.fetch_epl_matches(pool).await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        } else {
            tracing::warn!("FOOTBALL_DATA_API_KEY not set — skipping EPL");
        }

        if self.has_nba_key() {
            self.fetch_nba_teams(pool).await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            self.fetch_nba_games(pool).await?;
        } else {
            tracing::warn!("BALLDONTLIE_API_KEY not set — skipping NBA");
        }

        Ok(())
    }
}
