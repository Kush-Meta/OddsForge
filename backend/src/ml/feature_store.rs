use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::Row;
use crate::models::Match;

pub const N_FEATURES: usize = 26;

pub struct Features(pub [f64; N_FEATURES]);

pub async fn build_features(pool: &SqlitePool, m: &Match) -> Result<Features> {
    let mut f = [0.0f64; N_FEATURES];

    // ELO ratings
    let home_elo: f64 = sqlx::query("SELECT elo_rating FROM teams WHERE id = ?")
        .bind(&m.home_team_id)
        .fetch_optional(pool).await?
        .map(|r| r.get::<f64, _>("elo_rating"))
        .unwrap_or(1200.0);
    let away_elo: f64 = sqlx::query("SELECT elo_rating FROM teams WHERE id = ?")
        .bind(&m.away_team_id)
        .fetch_optional(pool).await?
        .map(|r| r.get::<f64, _>("elo_rating"))
        .unwrap_or(1200.0);

    f[0] = home_elo - away_elo;

    // Advanced stats
    let home_adv = sqlx::query("SELECT net_rating, off_rating, def_rating, efg_pct, tov_pct, oreb_pct, ft_rate, pace FROM nba_advanced_stats WHERE team_id = ?")
        .bind(&m.home_team_id).fetch_optional(pool).await?;
    let away_adv = sqlx::query("SELECT net_rating, off_rating, def_rating, efg_pct, tov_pct, oreb_pct, ft_rate, pace FROM nba_advanced_stats WHERE team_id = ?")
        .bind(&m.away_team_id).fetch_optional(pool).await?;

    if let Some(ref r) = home_adv {
        f[1] = r.get::<f64, _>("net_rating");
        f[3] = r.get::<f64, _>("off_rating");
        f[5] = r.get::<f64, _>("def_rating");
        f[7] = r.get::<f64, _>("efg_pct");
        f[9] = r.get::<f64, _>("tov_pct");
        f[11] = r.get::<f64, _>("oreb_pct");
        f[13] = r.get::<f64, _>("ft_rate");
        f[15] = r.get::<f64, _>("pace");
    } else {
        f[1] = 0.0; f[3] = 110.0; f[5] = 110.0; f[7] = 0.52;
        f[9] = 0.14; f[11] = 0.28; f[13] = 0.24; f[15] = 100.0;
    }
    if let Some(ref r) = away_adv {
        f[2] = r.get::<f64, _>("net_rating");
        f[4] = r.get::<f64, _>("off_rating");
        f[6] = r.get::<f64, _>("def_rating");
        f[8] = r.get::<f64, _>("efg_pct");
        f[10] = r.get::<f64, _>("tov_pct");
        f[12] = r.get::<f64, _>("oreb_pct");
        f[14] = r.get::<f64, _>("ft_rate");
        f[16] = r.get::<f64, _>("pace");
    } else {
        f[2] = 0.0; f[4] = 110.0; f[6] = 110.0; f[8] = 0.52;
        f[10] = 0.14; f[12] = 0.28; f[14] = 0.24; f[16] = 100.0;
    }

    // Win percentages from team_stats
    for (idx, team_id) in [(17usize, &m.home_team_id), (18, &m.away_team_id)] {
        let row = sqlx::query(
            "SELECT wins, matches_played FROM team_stats WHERE team_id = ? ORDER BY season DESC LIMIT 1"
        ).bind(team_id).fetch_optional(pool).await?;
        if let Some(r) = row {
            let w: i32 = r.get::<i32, _>("wins");
            let mp: i32 = r.get::<i32, _>("matches_played");
            f[idx] = if mp > 0 { w as f64 / mp as f64 } else { 0.5 };
        } else {
            f[idx] = 0.5;
        }
    }

    // H2H win rate for home team (regressed toward 0.55 with prior weight 3)
    // We count wins from home_team perspective: home team wins when they score more
    // regardless of which side they were on historically.
    let h2h = sqlx::query(
        r#"SELECT
            SUM(CASE WHEN (home_team_id = ? AND home_score > away_score)
                          OR (away_team_id = ? AND away_score > home_score) THEN 1 ELSE 0 END) as home_wins,
            COUNT(*) as total
           FROM matches
           WHERE status = 'finished' AND home_score IS NOT NULL
             AND ((home_team_id = ? AND away_team_id = ?) OR (home_team_id = ? AND away_team_id = ?))"#
    )
    .bind(&m.home_team_id).bind(&m.home_team_id)
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

    // Rest days & back-to-back flags
    for (rest_idx, b2b_idx, team_id) in [(20usize, 22usize, &m.home_team_id), (21, 23, &m.away_team_id)] {
        let last = sqlx::query(
            "SELECT match_date FROM matches WHERE (home_team_id = ? OR away_team_id = ?) AND status = 'finished' ORDER BY match_date DESC LIMIT 1"
        ).bind(team_id).bind(team_id).fetch_optional(pool).await?;

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

    // Opponent-adjusted form (last 10 finished games, avg point diff weighted by opponent ELO)
    for (form_idx, team_id) in [(24usize, &m.home_team_id), (25, &m.away_team_id)] {
        let rows = sqlx::query(
            r#"SELECT m.home_team_id, m.away_team_id, m.home_score, m.away_score,
                      ht.elo_rating as home_elo, at.elo_rating as away_elo
               FROM matches m
               JOIN teams ht ON ht.id = m.home_team_id
               JOIN teams at ON at.id = m.away_team_id
               WHERE (m.home_team_id = ? OR m.away_team_id = ?)
                 AND m.status = 'finished' AND m.home_score IS NOT NULL
               ORDER BY m.match_date DESC LIMIT 10"#
        ).bind(team_id).bind(team_id).fetch_all(pool).await?;

        if rows.is_empty() {
            f[form_idx] = 0.0;
        } else {
            let mut weighted_sum = 0.0f64;
            let mut weight_total = 0.0f64;
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
                weight_total += 1.0;
            }
            f[form_idx] = if weight_total > 0.0 { weighted_sum / weight_total } else { 0.0 };
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
