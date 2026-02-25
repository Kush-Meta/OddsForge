/// Fetches live odds from The Odds API and stores them in the `market_odds` table.
///
/// ## Credit budget (500 free req / month)
/// Each `refresh_odds_if_stale` call consumes at most **2 API requests** (1 per sport).
/// The function skips a sport if:
///   1. The last successful fetch for that sport was < 12 hours ago, OR
///   2. There are no upcoming matches for that sport in the next 3 days.
///
/// At 12-hour throttle: max 2 calls/sport/day × 2 sports × 30 days = **120 req/month**.
/// In practice far fewer, since EPL has no matches most weekdays.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};

use crate::db::upsert_market_odds;

// ── Odds API response types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OddsEvent {
    #[allow(dead_code)]
    id: String,
    commence_time: DateTime<Utc>,
    home_team: String,
    away_team: String,
    bookmakers: Vec<Bookmaker>,
}

#[derive(Debug, Deserialize)]
struct Bookmaker {
    key: String,
    title: String,
    markets: Vec<Market>,
}

#[derive(Debug, Deserialize)]
struct Market {
    key: String,
    outcomes: Vec<Outcome>,
}

#[derive(Debug, Deserialize)]
struct Outcome {
    name: String,
    price: f64,
}

struct BestOdds {
    home_odds: f64,
    draw_odds: Option<f64>,
    away_odds: f64,
    bookmaker: String,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Refresh odds for EPL and NBA if stale. Returns number of match odds upserted.
pub async fn refresh_odds_if_stale(pool: &SqlitePool, api_key: &str) -> u32 {
    let mut total = 0u32;

    // --- EPL ---
    if is_stale(pool, "soccer_epl").await && has_upcoming(pool, "football", 3).await {
        match fetch_sport(pool, api_key, "soccer_epl", "eu").await {
            Ok(n) => {
                total += n;
                tracing::info!("Odds: {} EPL events stored", n);
                mark_fetched(pool, "soccer_epl").await;
            }
            Err(e) => tracing::error!("Odds fetch failed (EPL): {}", e),
        }
    } else {
        tracing::debug!("Odds: EPL fetch skipped (not stale or no upcoming matches)");
    }

    // --- NBA ---
    if is_stale(pool, "basketball_nba").await && has_upcoming(pool, "basketball", 3).await {
        match fetch_sport(pool, api_key, "basketball_nba", "us").await {
            Ok(n) => {
                total += n;
                tracing::info!("Odds: {} NBA events stored", n);
                mark_fetched(pool, "basketball_nba").await;
            }
            Err(e) => tracing::error!("Odds fetch failed (NBA): {}", e),
        }
    } else {
        tracing::debug!("Odds: NBA fetch skipped (not stale or no upcoming matches)");
    }

    total
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Returns true if we haven't fetched this sport_key in the last 12 hours.
async fn is_stale(pool: &SqlitePool, sport_key: &str) -> bool {
    let last: Option<String> = sqlx::query_scalar(
        "SELECT last_fetched FROM odds_fetch_log WHERE sport_key = ?",
    )
    .bind(sport_key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match last {
        None => true,
        Some(ts) => {
            let fetched = DateTime::parse_from_rfc3339(&ts)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now() - Duration::hours(25));
            Utc::now().signed_duration_since(fetched) > Duration::hours(12)
        }
    }
}

async fn mark_fetched(pool: &SqlitePool, sport_key: &str) {
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT OR REPLACE INTO odds_fetch_log (sport_key, last_fetched) VALUES (?, ?)",
    )
    .bind(sport_key)
    .bind(&now)
    .execute(pool)
    .await;
}

/// Returns true if there are scheduled matches for `sport` starting within `days` days.
async fn has_upcoming(pool: &SqlitePool, sport: &str, days: i64) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches \
         WHERE sport = ? AND status = 'scheduled' \
           AND match_date > datetime('now') \
           AND match_date < datetime('now', ? || ' days')",
    )
    .bind(sport)
    .bind(days.to_string())
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    count > 0
}

/// Calls The Odds API for one sport and stores best odds for each matched event.
async fn fetch_sport(
    pool: &SqlitePool,
    api_key: &str,
    sport_key: &str,
    region: &str,
) -> Result<u32> {
    let url = format!(
        "https://api.the-odds-api.com/v4/sports/{}/odds/\
         ?apiKey={}&regions={}&markets=h2h&oddsFormat=decimal&dateFormat=iso",
        sport_key, api_key, region
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?;

    let status = resp.status();
    if status == 401 {
        return Err(anyhow::anyhow!("Odds API: invalid API key (401)"));
    }
    if status == 422 {
        return Err(anyhow::anyhow!("Odds API: sport {} not in subscription (422)", sport_key));
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Odds API HTTP {}: {}", status, body));
    }

    let events: Vec<OddsEvent> = resp.json().await?;
    let mut upserted = 0u32;

    for event in &events {
        let Some(odds) = best_odds(event) else { continue };

        // Match to our DB by kick-off time window (±4 h) + team name fuzzy match
        let Some(match_id) =
            find_match_id(pool, &event.home_team, &event.away_team, event.commence_time).await
        else {
            tracing::debug!(
                "Odds: no DB match for {} vs {} at {}",
                event.home_team, event.away_team, event.commence_time
            );
            continue;
        };

        if let Err(e) = upsert_market_odds(
            pool,
            &match_id,
            &odds.bookmaker,
            odds.home_odds,
            odds.draw_odds,
            odds.away_odds,
        )
        .await
        {
            tracing::error!("Odds upsert failed for match {}: {}", match_id, e);
        } else {
            upserted += 1;
        }
    }

    Ok(upserted)
}

/// Select the sharpest odds from a bookmaker priority list, fallback to lowest overround.
fn best_odds(event: &OddsEvent) -> Option<BestOdds> {
    let priority = ["pinnacle", "betfair_ex_eu", "betfair_ex_uk", "williamhill", "bet365"];

    let extract = |bk: &Bookmaker| -> Option<(f64, Option<f64>, f64)> {
        let market = bk.markets.iter().find(|m| m.key == "h2h")?;
        let home_price = market
            .outcomes
            .iter()
            .find(|o| names_match(&o.name, &event.home_team))
            .map(|o| o.price)?;
        let away_price = market
            .outcomes
            .iter()
            .find(|o| names_match(&o.name, &event.away_team))
            .map(|o| o.price)?;
        let draw_price = market
            .outcomes
            .iter()
            .find(|o| o.name.to_lowercase() == "draw")
            .map(|o| o.price);
        if home_price > 1.0 && away_price > 1.0 {
            Some((home_price, draw_price, away_price))
        } else {
            None
        }
    };

    // 1. Try priority (sharpest) books first
    for pref in &priority {
        if let Some(bk) = event.bookmakers.iter().find(|b| b.key == *pref) {
            if let Some((h, d, a)) = extract(bk) {
                return Some(BestOdds {
                    home_odds: h,
                    draw_odds: d,
                    away_odds: a,
                    bookmaker: bk.title.clone(),
                });
            }
        }
    }

    // 2. Fallback: lowest overround across all bookmakers
    event
        .bookmakers
        .iter()
        .filter_map(|bk| {
            let (h, d, a) = extract(bk)?;
            let overround = 1.0 / h + d.map(|x| 1.0 / x).unwrap_or(0.0) + 1.0 / a;
            Some((h, d, a, overround, bk.title.clone()))
        })
        .min_by(|x, y| x.3.partial_cmp(&y.3).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(h, d, a, _, name)| BestOdds {
            home_odds: h,
            draw_odds: d,
            away_odds: a,
            bookmaker: name,
        })
}

/// Find our internal match_id by matching team names and kick-off time.
async fn find_match_id(
    pool: &SqlitePool,
    home_team: &str,
    away_team: &str,
    commence_time: DateTime<Utc>,
) -> Option<String> {
    // Look for scheduled matches within ±4 hours of the commence_time
    let window_start = (commence_time - Duration::hours(4)).to_rfc3339();
    let window_end = (commence_time + Duration::hours(4)).to_rfc3339();

    let rows = sqlx::query(
        "SELECT id, home_team_name, away_team_name FROM matches \
         WHERE status = 'scheduled' AND match_date BETWEEN ? AND ?",
    )
    .bind(&window_start)
    .bind(&window_end)
    .fetch_all(pool)
    .await
    .ok()?;

    for row in rows {
        let id: String = row.get("id");
        let db_home: String = row.get("home_team_name");
        let db_away: String = row.get("away_team_name");

        if names_match(&db_home, home_team) && names_match(&db_away, away_team) {
            return Some(id);
        }
    }
    None
}

/// Fuzzy team-name match: normalises common suffixes then checks contains-both-ways.
fn names_match(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> String {
        s.to_lowercase()
            .replace(" fc", "")
            .replace("fc ", "")
            .replace("afc ", "")
            .replace(" afc", "")
            .replace(" sc", "")
            .replace(".", "")
            .replace("-", " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let a = norm(a);
    let b = norm(b);
    a == b || a.contains(&b) || b.contains(&a)
}
