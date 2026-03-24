//! Fetches advanced team stats from the unofficial NBA Stats API (stats.nba.com).
//!
//! No API key required, but the endpoint needs browser-like headers to avoid 403s.
//! We fetch two measure types per refresh cycle:
//!   - "Advanced"     → ORtg, DRtg, NetRtg, Pace
//!   - "Four+Factors" → eFG%, TOV%, OREB%, FTr (both sides of the ball)
//!
//! Data is stored in `nba_advanced_stats` and refreshed at most every 6 hours.

use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

use crate::db::upsert_nba_advanced_stats;
use crate::models::NbaAdvancedStats;

const NBA_STATS_BASE: &str = "https://stats.nba.com/stats";
pub const CURRENT_SEASON: &str = "2025-26";
/// Minimum hours between refreshes — avoids hammering the unofficial API.
const MIN_REFRESH_HOURS: i64 = 6;

pub struct NbaStatsFetcher {
    client: Client,
}

impl NbaStatsFetcher {
    pub fn new() -> Result<Self> {
        use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, ORIGIN, REFERER};

        let mut headers = HeaderMap::new();
        headers.insert(REFERER,          HeaderValue::from_static("https://www.nba.com/"));
        headers.insert(ACCEPT,           HeaderValue::from_static("*/*"));
        headers.insert(ACCEPT_LANGUAGE,  HeaderValue::from_static("en-US,en;q=0.9"));
        headers.insert(ORIGIN,           HeaderValue::from_static("https://www.nba.com"));
        headers.insert(
            "x-nba-stats-origin",
            HeaderValue::from_static("stats"),
        );
        headers.insert(
            "x-nba-stats-token",
            HeaderValue::from_static("true"),
        );

        let client = Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                 AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/123.0.0.0 Safari/537.36",
            )
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { client })
    }

    /// Returns true when the stored stats are absent or older than MIN_REFRESH_HOURS.
    pub async fn should_refresh(pool: &SqlitePool) -> bool {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT MAX(fetched_at) FROM nba_advanced_stats",
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        match result {
            None => true,
            Some(ts) => match chrono::DateTime::parse_from_rfc3339(&ts) {
                Ok(dt) => (Utc::now() - dt.with_timezone(&Utc)).num_hours() >= MIN_REFRESH_HOURS,
                Err(_) => true,
            },
        }
    }

    /// Fetch both measure types, merge by team name, and upsert into the DB.
    /// Returns the number of teams successfully stored, or 0 on API failure
    /// (caller continues with whatever is already in the DB).
    pub async fn fetch_and_store(&self, pool: &SqlitePool) -> Result<usize> {
        tracing::info!("Fetching NBA advanced stats from stats.nba.com (season {})…", CURRENT_SEASON);

        let advanced = match self.fetch_measure_type("Advanced").await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("NBA Stats API (Advanced) unavailable: {}", e);
                return Ok(0);
            }
        };

        // Small delay to avoid back-to-back requests looking suspicious
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        let four_factors = match self.fetch_measure_type("Four+Factors").await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("NBA Stats API (Four+Factors) unavailable: {}", e);
                return Ok(0);
            }
        };

        // Build lookup: team_name → advanced row
        let adv_map: HashMap<String, HashMap<String, Value>> = advanced
            .into_iter()
            .filter_map(|row| {
                let name = row.get("TEAM_NAME")?.as_str()?.to_string();
                Some((name, row))
            })
            .collect();

        // Load our DB teams for name → id matching
        let db_teams: Vec<(String, String)> = sqlx::query(
            "SELECT id, name FROM teams WHERE sport = 'basketball'",
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|row| {
            let id: String = row.try_get("id").ok()?;
            let name: String = row.try_get("name").ok()?;
            Some((id, name))
        })
        .collect();

        let now = Utc::now().to_rfc3339();
        let mut stored = 0usize;

        for ff_row in &four_factors {
            let team_name = match ff_row.get("TEAM_NAME").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => continue,
            };

            let adv = match adv_map.get(team_name) {
                Some(r) => r,
                None => {
                    tracing::debug!("No advanced stats row for '{}'", team_name);
                    continue;
                }
            };

            let team_id = match find_team_id(&db_teams, team_name) {
                Some(id) => id,
                None => {
                    tracing::debug!("No DB team matched for '{}'", team_name);
                    continue;
                }
            };

            let stats = NbaAdvancedStats {
                team_id,
                off_rating:   get_f64(adv, "OFF_RATING").unwrap_or(110.0),
                def_rating:   get_f64(adv, "DEF_RATING").unwrap_or(110.0),
                net_rating:   get_f64(adv, "NET_RATING").unwrap_or(0.0),
                pace:         get_f64(adv, "PACE").unwrap_or(100.0),
                efg_pct:      get_f64(ff_row, "EFG_PCT").unwrap_or(0.52),
                opp_efg_pct:  get_f64(ff_row, "OPP_EFG_PCT").unwrap_or(0.52),
                tov_pct:      get_f64(ff_row, "TM_TOV_PCT").unwrap_or(0.14),
                opp_tov_pct:  get_f64(ff_row, "OPP_TOV_PCT").unwrap_or(0.14),
                oreb_pct:     get_f64(ff_row, "OREB_PCT").unwrap_or(0.28),
                opp_oreb_pct: get_f64(ff_row, "OPP_OREB_PCT").unwrap_or(0.28),
                ft_rate:      get_f64(ff_row, "FTA_RATE").unwrap_or(0.24),
                opp_ft_rate:  get_f64(ff_row, "OPP_FTA_RATE").unwrap_or(0.24),
                games_played: get_i32(adv, "GP").unwrap_or(0),
                wins:         get_i32(adv, "W").unwrap_or(0),
                season:       CURRENT_SEASON.to_string(),
                fetched_at:   now.clone(),
            };

            upsert_nba_advanced_stats(pool, &stats).await?;
            stored += 1;
        }

        tracing::info!("NBA advanced stats stored for {} teams", stored);
        Ok(stored)
    }

    async fn fetch_measure_type(
        &self,
        measure_type: &str,
    ) -> Result<Vec<HashMap<String, Value>>> {
        let url = format!(
            "{}/leaguedashteamstats\
             ?Season={}&SeasonType=Regular+Season&MeasureType={}\
             &PerMode=PerGame&PaceAdjust=N&PlusMinus=N&Rank=N\
             &LastNGames=0&Month=0&OpponentTeamID=0&Period=0&PORound=0&TwoWay=0",
            NBA_STATS_BASE, CURRENT_SEASON, measure_type
        );

        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(anyhow!(
                "NBA Stats API ({}) HTTP {}", measure_type, resp.status()
            ));
        }

        let json: Value = resp.json().await?;
        parse_result_set(&json)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Convert `{"resultSets":[{"headers":[…],"rowSet":[[…],…]}]}` to a flat
/// `Vec<HashMap<header_name, json_value>>`.
fn parse_result_set(json: &Value) -> Result<Vec<HashMap<String, Value>>> {
    let result_set = json["resultSets"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| anyhow!("Missing resultSets in NBA Stats response"))?;

    let headers: Vec<String> = result_set["headers"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing headers"))?
        .iter()
        .filter_map(|h| h.as_str().map(|s| s.to_string()))
        .collect();

    let rows = result_set["rowSet"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing rowSet"))?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let cells = row.as_array()?;
            let mut map = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                if let Some(cell) = cells.get(i) {
                    map.insert(header.clone(), cell.clone());
                }
            }
            Some(map)
        })
        .collect())
}

/// Match an NBA Stats API team name to an internal DB team ID.
/// Strategy: exact match → nickname (last word) match → strsim best match.
fn find_team_id(db_teams: &[(String, String)], api_name: &str) -> Option<String> {
    let norm = api_name.trim().to_lowercase();

    // 1. Exact (case-insensitive) match
    for (id, name) in db_teams {
        if name.trim().to_lowercase() == norm {
            return Some(id.clone());
        }
    }

    // 2. Nickname match: last word, e.g., "Hawks" matches "Atlanta Hawks"
    let api_suffix = norm.split_whitespace().last().unwrap_or("");
    for (id, name) in db_teams {
        let db_norm = name.trim().to_lowercase();
        if db_norm.split_whitespace().last().unwrap_or("") == api_suffix {
            return Some(id.clone());
        }
    }

    // 3. Fuzzy match using strsim (handles rare abbreviation differences)
    let best = db_teams
        .iter()
        .map(|(id, name)| {
            let score = strsim::jaro_winkler(&norm, &name.to_lowercase());
            (id.clone(), score)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    if let Some((id, score)) = best {
        if score > 0.85 {
            return Some(id);
        }
    }

    None
}

fn get_f64(map: &HashMap<String, Value>, key: &str) -> Option<f64> {
    map.get(key).and_then(|v| v.as_f64())
}

fn get_i32(map: &HashMap<String, Value>, key: &str) -> Option<i32> {
    map.get(key).and_then(|v| v.as_i64()).map(|i| i as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_result_set_basic() {
        let json = json!({
            "resultSets": [{
                "headers": ["TEAM_ID", "TEAM_NAME", "NET_RATING"],
                "rowSet": [
                    [1610612737, "Atlanta Hawks", 2.5],
                    [1610612738, "Boston Celtics", 8.1]
                ]
            }]
        });
        let rows = parse_result_set(&json).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["TEAM_NAME"].as_str().unwrap(), "Atlanta Hawks");
        assert!((rows[1]["NET_RATING"].as_f64().unwrap() - 8.1).abs() < 0.001);
    }

    #[test]
    fn test_find_team_id_exact() {
        let db_teams = vec![
            ("nba_1".to_string(), "Atlanta Hawks".to_string()),
            ("nba_2".to_string(), "Boston Celtics".to_string()),
        ];
        assert_eq!(find_team_id(&db_teams, "Atlanta Hawks"), Some("nba_1".to_string()));
        assert_eq!(find_team_id(&db_teams, "Boston Celtics"), Some("nba_2".to_string()));
    }

    #[test]
    fn test_find_team_id_nickname_fallback() {
        let db_teams = vec![
            ("nba_1".to_string(), "Atlanta Hawks".to_string()),
            ("nba_2".to_string(), "Boston Celtics".to_string()),
        ];
        // API sometimes omits city
        assert_eq!(find_team_id(&db_teams, "Hawks"), Some("nba_1".to_string()));
    }

    #[test]
    fn test_find_team_id_fuzzy_fallback() {
        let db_teams = vec![
            ("nba_1".to_string(), "Los Angeles Lakers".to_string()),
            ("nba_2".to_string(), "Los Angeles Clippers".to_string()),
        ];
        assert_eq!(find_team_id(&db_teams, "LA Lakers"), Some("nba_1".to_string()));
    }

    #[test]
    fn test_find_team_id_no_match() {
        let db_teams = vec![("nba_1".to_string(), "Atlanta Hawks".to_string())];
        assert_eq!(find_team_id(&db_teams, "XXXXXX Unknown"), None);
    }
}
