use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::Row;
use crate::models::Match;

pub const N_FEATURES: usize = 26;

pub struct Features(pub [f64; N_FEATURES]);

/// Rolling Four Factors computed from the last `n` games in game_box_stats before `before_date`.
struct RollingFactors {
    efg_pct: f64,  // (FGM + 0.5*FG3M) / FGA
    tov_pct: f64,  // TOV / (FGA + 0.44*FTA + TOV)
    oreb_pg: f64,  // offensive rebounds per game
    oreb_pct: f64, // OREB / (OREB + opp DREB)  — approximated as OREB / (OREB + DREB) same game
    ftr: f64,      // FTA / FGA
    pace: f64,     // possessions per game ≈ FGA - OREB + TOV + 0.44*FTA
    pts_pg: f64,   // points per game
}

impl Default for RollingFactors {
    fn default() -> Self {
        Self { efg_pct: 0.52, tov_pct: 0.14, oreb_pg: 10.5, oreb_pct: 0.28, ftr: 0.24, pace: 100.0, pts_pg: 110.0 }
    }
}

async fn rolling_factors(pool: &SqlitePool, team_id: &str, before_date: &str, n: i64) -> RollingFactors {
    let rows = sqlx::query(
        "SELECT pts, fgm, fga, fg3m, fg3a, ftm, fta, oreb, dreb, tov
         FROM game_box_stats
         WHERE team_id = ? AND game_date < ?
         ORDER BY game_date DESC LIMIT ?"
    )
    .bind(team_id).bind(before_date).bind(n)
    .fetch_all(pool).await.unwrap_or_default();

    if rows.is_empty() {
        return RollingFactors::default();
    }

    let ng = rows.len() as f64;
    let (mut pts, mut fgm, mut fga, mut fg3m, mut ftm, mut fta, mut oreb, mut dreb, mut tov) =
        (0f64, 0f64, 0f64, 0f64, 0f64, 0f64, 0f64, 0f64, 0f64);

    for r in &rows {
        pts  += r.get::<f64, _>("pts");
        fgm  += r.get::<f64, _>("fgm");
        fga  += r.get::<f64, _>("fga");
        fg3m += r.get::<f64, _>("fg3m");
        ftm  += r.get::<f64, _>("ftm");
        fta  += r.get::<f64, _>("fta");
        oreb += r.get::<f64, _>("oreb");
        dreb += r.get::<f64, _>("dreb");
        tov  += r.get::<f64, _>("tov");
    }

    let efg_pct = if fga > 0.0 { (fgm + 0.5 * fg3m) / fga } else { 0.52 };
    let tov_denom = fga + 0.44 * fta + tov;
    let tov_pct = if tov_denom > 0.0 { tov / tov_denom } else { 0.14 };
    let ftr = if fga > 0.0 { fta / fga } else { 0.24 };
    // OREB% = OREB / (OREB + opp_DREB). Approximate with own DREB as proxy for opp OREB (team rebounds ~= opp misses)
    let oreb_pct = if (oreb + dreb) > 0.0 { oreb / (oreb + dreb) } else { 0.28 };
    let oreb_pg = oreb / ng;
    let pace = (fga - oreb / ng + tov / ng + 0.44 * fta / ng).max(70.0).min(130.0);
    let pts_pg = pts / ng;

    RollingFactors { efg_pct, tov_pct, oreb_pg, oreb_pct, ftr, pace, pts_pg }
}

/// Look up a team's ELO at game time from elo_history, falling back to current ELO.
async fn historical_elo(pool: &SqlitePool, team_id: &str, before_date: &str) -> f64 {
    // Try elo_history first — gives ELO at the time of the game
    if let Ok(Some(row)) = sqlx::query(
        "SELECT elo_rating FROM elo_history WHERE team_id = ? AND date <= ? ORDER BY date DESC LIMIT 1"
    )
    .bind(team_id).bind(before_date)
    .fetch_optional(pool).await
    {
        return row.get::<f64, _>("elo_rating");
    }
    // Fallback: current ELO
    sqlx::query("SELECT elo_rating FROM teams WHERE id = ?")
        .bind(team_id)
        .fetch_optional(pool).await
        .ok().flatten()
        .map(|r| r.get::<f64, _>("elo_rating"))
        .unwrap_or(1200.0)
}

pub async fn build_features(pool: &SqlitePool, m: &Match) -> Result<Features> {
    let mut f = [0.0f64; N_FEATURES];

    // Date string for "before this game" queries
    let before_date = m.match_date.format("%Y-%m-%d").to_string();

    // ── f[0]: ELO differential (historical at game time) ─────────────────────
    let (home_elo, away_elo) = tokio::join!(
        historical_elo(pool, &m.home_team_id, &before_date),
        historical_elo(pool, &m.away_team_id, &before_date),
    );
    f[0] = home_elo - away_elo;

    // ── f[1]–f[16]: Rolling Four Factors (last 10 games before this game) ────
    // Uses game_box_stats when available (post-ingest); falls back to nba_advanced_stats
    // or league defaults if no box score data exists.
    let (home_rf, away_rf) = tokio::join!(
        rolling_factors(pool, &m.home_team_id, &before_date, 10),
        rolling_factors(pool, &m.away_team_id, &before_date, 10),
    );

    // Net rating proxy: pts_pg − opp_pts_pg. We store pts_pg; opp is away's pts_pg.
    // Use the delta as a net rating approximation.
    let home_net = home_rf.pts_pg - away_rf.pts_pg;
    let away_net = away_rf.pts_pg - home_rf.pts_pg;

    f[1]  = home_net;          // home net rating proxy
    f[2]  = away_net;          // away net rating proxy
    f[3]  = home_rf.pts_pg;    // home off rating proxy (pts/game)
    f[4]  = away_rf.pts_pg;    // away off rating proxy
    f[5]  = 110.0 - home_rf.pts_pg; // home def rating proxy (crude)
    f[6]  = 110.0 - away_rf.pts_pg;
    f[7]  = home_rf.efg_pct;
    f[8]  = away_rf.efg_pct;
    f[9]  = home_rf.tov_pct;
    f[10] = away_rf.tov_pct;
    f[11] = home_rf.oreb_pct;
    f[12] = away_rf.oreb_pct;
    f[13] = home_rf.ftr;
    f[14] = away_rf.ftr;
    f[15] = home_rf.pace;
    f[16] = away_rf.pace;

    // ── f[17]–f[18]: Win% from rolling box stats context ─────────────────────
    // Count wins in last 20 games before this date
    for (idx, team_id) in [(17usize, &m.home_team_id), (18, &m.away_team_id)] {
        let wins: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM matches
               WHERE status = 'finished' AND match_date < ?
                 AND home_score IS NOT NULL AND away_score IS NOT NULL
                 AND (
                   (home_team_id = ? AND home_score > away_score) OR
                   (away_team_id = ? AND away_score > home_score)
                 )
               ORDER BY match_date DESC LIMIT 20"#
        ).bind(&before_date).bind(team_id).bind(team_id)
        .fetch_one(pool).await.unwrap_or(0);

        let played: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM matches WHERE status = 'finished' AND match_date < ? AND (home_team_id = ? OR away_team_id = ?) ORDER BY match_date DESC LIMIT 20"
        ).bind(&before_date).bind(team_id).bind(team_id)
        .fetch_one(pool).await.unwrap_or(0);

        f[idx] = if played > 0 { wins as f64 / played as f64 } else { 0.5 };
    }

    // ── f[19]: H2H home team win rate (regressed) ────────────────────────────
    let h2h = sqlx::query(
        r#"SELECT
            SUM(CASE WHEN (home_team_id = ? AND home_score > away_score)
                          OR (away_team_id = ? AND away_score > home_score) THEN 1 ELSE 0 END) as home_wins,
            COUNT(*) as total
           FROM matches
           WHERE status = 'finished' AND home_score IS NOT NULL AND match_date < ?
             AND ((home_team_id = ? AND away_team_id = ?) OR (home_team_id = ? AND away_team_id = ?))"#
    )
    .bind(&m.home_team_id).bind(&m.home_team_id)
    .bind(&before_date)
    .bind(&m.home_team_id).bind(&m.away_team_id)
    .bind(&m.away_team_id).bind(&m.home_team_id)
    .fetch_optional(pool).await?;

    f[19] = if let Some(r) = h2h {
        let hw: i64 = r.get::<i64, _>("home_wins");
        let tot: i64 = r.get::<i64, _>("total");
        (hw as f64 + 3.0 * 0.55) / (tot as f64 + 3.0)
    } else {
        0.55
    };

    // ── f[20]–f[23]: Rest days & back-to-back ────────────────────────────────
    for (rest_idx, b2b_idx, team_id) in [(20usize, 22usize, &m.home_team_id), (21, 23, &m.away_team_id)] {
        let last = sqlx::query(
            "SELECT match_date FROM matches WHERE (home_team_id = ? OR away_team_id = ?) AND status = 'finished' AND match_date < ? ORDER BY match_date DESC LIMIT 1"
        ).bind(team_id).bind(team_id).bind(m.match_date.to_rfc3339())
        .fetch_optional(pool).await?;

        let rest = if let Some(r) = last {
            let date_str: String = r.get("match_date");
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&date_str) {
                let diff = m.match_date.signed_duration_since(dt.with_timezone(&chrono::Utc));
                (diff.num_hours() as f64 / 24.0).max(0.0).min(7.0)
            } else { 3.0 }
        } else { 3.0 };
        f[rest_idx] = rest;
        f[b2b_idx] = if rest < 1.5 { 1.0 } else { 0.0 };
    }

    // ── f[24]–f[25]: Opponent-adjusted form (last 10 games, ELO-weighted) ────
    for (form_idx, team_id) in [(24usize, &m.home_team_id), (25, &m.away_team_id)] {
        let rows = sqlx::query(
            r#"SELECT m.home_team_id, m.away_team_id, m.home_score, m.away_score,
                      ht.elo_rating as home_elo, at.elo_rating as away_elo
               FROM matches m
               JOIN teams ht ON ht.id = m.home_team_id
               JOIN teams at ON at.id = m.away_team_id
               WHERE (m.home_team_id = ? OR m.away_team_id = ?)
                 AND m.status = 'finished' AND m.home_score IS NOT NULL
                 AND m.match_date < ?
               ORDER BY m.match_date DESC LIMIT 10"#
        ).bind(team_id).bind(team_id).bind(m.match_date.to_rfc3339())
        .fetch_all(pool).await?;

        if rows.is_empty() {
            f[form_idx] = 0.0;
        } else {
            let mut weighted_sum = 0.0f64;
            let avg_elo = 1200.0f64;
            for r in &rows {
                let home_id: String = r.get("home_team_id");
                let hs: i32 = r.get("home_score");
                let aws: i32 = r.get("away_score");
                let h_elo: f64 = r.get("home_elo");
                let a_elo: f64 = r.get("away_elo");
                let (margin, opp_elo) = if &home_id == team_id {
                    ((hs - aws) as f64, a_elo)
                } else {
                    ((aws - hs) as f64, h_elo)
                };
                let sos_weight = 1.0 + (opp_elo - avg_elo) / 400.0;
                weighted_sum += margin * sos_weight.max(0.5).min(2.0);
            }
            f[form_idx] = weighted_sum / rows.len() as f64;
        }
    }

    Ok(Features(f))
}

/// Upsert features to ml_features table as JSON
pub async fn cache_features(pool: &SqlitePool, match_id: &str, features: &Features) -> Result<()> {
    let json = serde_json::to_string(&features.0.to_vec())?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO ml_features (match_id, features_json, computed_at) VALUES (?, ?, ?)"
    )
    .bind(match_id).bind(&json).bind(&now)
    .execute(pool).await?;
    Ok(())
}

/// Load cached features or compute them
pub async fn get_or_build_features(pool: &SqlitePool, m: &Match) -> Result<Features> {
    let cached = sqlx::query(
        "SELECT features_json FROM ml_features WHERE match_id = ?"
    ).bind(&m.id).fetch_optional(pool).await?;

    if let Some(row) = cached {
        let json: String = row.get("features_json");
        let vec: Vec<f64> = serde_json::from_str(&json)?;
        if vec.len() == N_FEATURES {
            let mut arr = [0.0f64; N_FEATURES];
            arr.copy_from_slice(&vec);
            return Ok(Features(arr));
        }
    }
    let f = build_features(pool, m).await?;
    cache_features(pool, &m.id, &f).await?;
    Ok(f)
}
