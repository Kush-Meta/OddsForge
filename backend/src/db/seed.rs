use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{Match, Prediction, Team, TeamStats};

/// Compute EPL win probabilities using ELO ratings with home advantage
fn epl_probs(home_elo: f64, away_elo: f64) -> (f64, f64, f64) {
    let adjusted = home_elo + 100.0;
    let expected_home = 1.0 / (1.0 + 10f64.powf((away_elo - adjusted) / 400.0));
    let draw = 0.25_f64;
    let home = expected_home * (1.0 - draw);
    let away = (1.0 - expected_home) * (1.0 - draw);
    let total = home + away + draw;
    (home / total, away / total, draw / total)
}

/// Compute NBA win probabilities (no draws)
fn nba_probs(home_elo: f64, away_elo: f64) -> (f64, f64) {
    let adjusted = home_elo + 100.0;
    let home = 1.0 / (1.0 + 10f64.powf((away_elo - adjusted) / 400.0));
    (home, 1.0 - home)
}

/// Confidence based on ELO difference
fn confidence(elo_diff: f64) -> f64 {
    let base = 0.60_f64;
    let bonus = (elo_diff.abs() / 800.0).min(0.30);
    base + bonus
}

async fn insert_team_raw(pool: &SqlitePool, team: &Team) -> Result<()> {
    sqlx::query(
        r#"INSERT OR REPLACE INTO teams (id,name,sport,league,logo_url,elo_rating,created_at,updated_at)
           VALUES (?,?,?,?,?,?,?,?)"#,
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

async fn insert_match_raw(pool: &SqlitePool, m: &Match) -> Result<()> {
    sqlx::query(
        r#"INSERT OR REPLACE INTO matches
           (id,home_team_id,away_team_id,home_team_name,away_team_name,sport,league,match_date,status,home_score,away_score,created_at,updated_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(&m.id)
    .bind(&m.home_team_id)
    .bind(&m.away_team_id)
    .bind(&m.home_team_name)
    .bind(&m.away_team_name)
    .bind(&m.sport)
    .bind(&m.league)
    .bind(m.match_date.to_rfc3339())
    .bind(&m.status)
    .bind(m.home_score)
    .bind(m.away_score)
    .bind(m.created_at.to_rfc3339())
    .bind(m.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_prediction_raw(pool: &SqlitePool, p: &Prediction) -> Result<()> {
    sqlx::query(
        r#"INSERT OR REPLACE INTO predictions
           (id,match_id,home_win_probability,away_win_probability,draw_probability,model_version,confidence_score,created_at)
           VALUES (?,?,?,?,?,?,?,?)"#,
    )
    .bind(&p.id)
    .bind(&p.match_id)
    .bind(p.home_win_probability)
    .bind(p.away_win_probability)
    .bind(p.draw_probability)
    .bind(&p.model_version)
    .bind(p.confidence_score)
    .bind(p.created_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_team_stats_raw(pool: &SqlitePool, s: &TeamStats) -> Result<()> {
    sqlx::query(
        r#"INSERT OR REPLACE INTO team_stats
           (id,team_id,season,matches_played,wins,draws,losses,goals_for,goals_against,points_for,points_against,form,updated_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(&s.id)
    .bind(&s.team_id)
    .bind(&s.season)
    .bind(s.matches_played)
    .bind(s.wins)
    .bind(s.draws)
    .bind(s.losses)
    .bind(s.goals_for)
    .bind(s.goals_against)
    .bind(s.points_for)
    .bind(s.points_against)
    .bind(&s.form)
    .bind(s.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn seed_data(pool: &SqlitePool) -> Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teams")
        .fetch_one(pool)
        .await?;

    if count > 0 {
        tracing::info!("Database already seeded ({} teams found), skipping.", count);
        return Ok(());
    }

    tracing::info!("Seeding database with EPL and NBA data...");

    seed_epl(pool).await?;
    seed_nba(pool).await?;

    tracing::info!("Database seeded successfully.");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  EPL
// ─────────────────────────────────────────────────────────────────────────────

async fn seed_epl(pool: &SqlitePool) -> Result<()> {
    let now = Utc::now();

    // (id, name, elo, wins, draws, losses, goals_for, goals_against, form)
    let teams: Vec<(&str, &str, f64, i32, i32, i32, i32, i32, &str)> = vec![
        ("epl_1",  "Arsenal",               1490.0, 18, 4, 4,  58, 26, "WWDWW"),
        ("epl_2",  "Liverpool",             1480.0, 19, 3, 4,  62, 29, "WWWLW"),
        ("epl_3",  "Manchester City",       1510.0, 17, 5, 4,  55, 30, "DWWWW"),
        ("epl_4",  "Chelsea",               1380.0, 14, 6, 6,  52, 38, "WLWWL"),
        ("epl_5",  "Aston Villa",           1370.0, 14, 5, 7,  47, 40, "WDWLD"),
        ("epl_6",  "Tottenham Hotspur",     1360.0, 13, 6, 7,  49, 44, "LWWDW"),
        ("epl_7",  "Newcastle United",      1340.0, 13, 5, 8,  44, 38, "WDLWW"),
        ("epl_8",  "Manchester United",     1330.0, 11, 7, 8,  35, 37, "DLDWW"),
        ("epl_9",  "Brighton",              1310.0, 11, 6, 9,  44, 42, "WDLLD"),
        ("epl_10", "West Ham United",       1290.0, 10, 6, 10, 38, 46, "LDWDL"),
        ("epl_11", "Everton",               1280.0,  9, 7, 10, 32, 40, "DLWLL"),
        ("epl_12", "Fulham",                1270.0, 10, 4, 12, 39, 48, "LWDLW"),
        ("epl_13", "Crystal Palace",        1260.0,  8, 8, 10, 33, 42, "DWLDD"),
        ("epl_14", "Brentford",             1260.0,  9, 5, 12, 38, 50, "LWDWL"),
        ("epl_15", "Wolves",                1250.0,  7, 8, 11, 30, 44, "DLLWD"),
        ("epl_16", "Nottingham Forest",     1250.0,  9, 4, 13, 32, 46, "WLLWD"),
        ("epl_17", "Bournemouth",           1240.0,  9, 4, 13, 37, 51, "LLLWW"),
        ("epl_18", "Leicester City",        1220.0,  6, 5, 15, 28, 55, "LLDLL"),
        ("epl_19", "Ipswich Town",          1210.0,  5, 5, 16, 24, 57, "LLLWL"),
        ("epl_20", "Southampton",           1200.0,  3, 4, 19, 20, 68, "LLLLL"),
    ];

    for (id, name, elo, w, d, l, gf, ga, form) in &teams {
        let team = Team {
            id: id.to_string(),
            name: name.to_string(),
            sport: "football".to_string(),
            league: "EPL".to_string(),
            logo_url: None,
            elo_rating: *elo,
            created_at: now,
            updated_at: now,
        };
        insert_team_raw(pool, &team).await?;

        let stats = TeamStats {
            id: Uuid::new_v4().to_string(),
            team_id: id.to_string(),
            season: "2025-26".to_string(),
            matches_played: w + d + l,
            wins: *w,
            draws: Some(*d),
            losses: *l,
            goals_for: Some(*gf),
            goals_against: Some(*ga),
            points_for: None,
            points_against: None,
            form: form.to_string(),
            updated_at: now,
        };
        insert_team_stats_raw(pool, &stats).await?;
    }

    // Build a lookup map: id -> elo
    let elo_map: std::collections::HashMap<&str, f64> =
        teams.iter().map(|(id, _, elo, ..)| (*id, *elo)).collect();
    let name_map: std::collections::HashMap<&str, &str> =
        teams.iter().map(|(id, name, ..)| (*id, *name)).collect();

    // ── Historical matches (finished) ──────────────────────────────────────
    let historical: Vec<(&str, &str, &str, &str, i32, i32, &str)> = vec![
        // match_id, home_id, away_id, date_str, home_score, away_score, status
        ("epl_h1",  "epl_1",  "epl_15", "2025-08-16T19:30:00Z", 2, 1, "finished"),
        ("epl_h2",  "epl_2",  "epl_14", "2025-08-17T14:00:00Z", 3, 1, "finished"),
        ("epl_h3",  "epl_3",  "epl_19", "2025-08-24T14:00:00Z", 4, 0, "finished"),
        ("epl_h4",  "epl_4",  "epl_13", "2025-08-24T16:30:00Z", 2, 2, "finished"),
        ("epl_h5",  "epl_1",  "epl_6",  "2025-09-14T15:30:00Z", 3, 2, "finished"),
        ("epl_h6",  "epl_2",  "epl_7",  "2025-09-21T15:30:00Z", 2, 0, "finished"),
        ("epl_h7",  "epl_3",  "epl_12", "2025-10-05T14:00:00Z", 3, 1, "finished"),
        ("epl_h8",  "epl_4",  "epl_1",  "2025-10-19T15:30:00Z", 1, 2, "finished"),
        ("epl_h9",  "epl_6",  "epl_11", "2025-11-02T14:00:00Z", 3, 0, "finished"),
        ("epl_h10", "epl_2",  "epl_3",  "2025-11-23T15:30:00Z", 3, 2, "finished"),
        ("epl_h11", "epl_1",  "epl_9",  "2025-12-07T14:00:00Z", 2, 1, "finished"),
        ("epl_h12", "epl_3",  "epl_2",  "2025-12-26T12:30:00Z", 2, 1, "finished"),
        ("epl_h13", "epl_4",  "epl_6",  "2026-01-04T14:00:00Z", 0, 1, "finished"),
        ("epl_h14", "epl_1",  "epl_15", "2026-01-18T14:00:00Z", 3, 0, "finished"),
        ("epl_h15", "epl_2",  "epl_11", "2026-02-08T14:00:00Z", 2, 1, "finished"),
        ("epl_h16", "epl_5",  "epl_8",  "2025-09-01T14:00:00Z", 2, 0, "finished"),
        ("epl_h17", "epl_7",  "epl_3",  "2025-10-26T14:00:00Z", 0, 2, "finished"),
        ("epl_h18", "epl_8",  "epl_2",  "2025-11-09T14:00:00Z", 1, 3, "finished"),
        ("epl_h19", "epl_6",  "epl_5",  "2025-12-15T14:00:00Z", 2, 2, "finished"),
        ("epl_h20", "epl_9",  "epl_4",  "2026-02-01T14:00:00Z", 1, 1, "finished"),
    ];

    for (mid, hid, aid, date_str, hs, as_, status) in &historical {
        let match_date = chrono::DateTime::parse_from_rfc3339(date_str)
            .unwrap()
            .with_timezone(&Utc);
        let m = Match {
            id: mid.to_string(),
            home_team_id: hid.to_string(),
            away_team_id: aid.to_string(),
            home_team_name: name_map[hid].to_string(),
            away_team_name: name_map[aid].to_string(),
            sport: "football".to_string(),
            league: "EPL".to_string(),
            match_date,
            status: status.to_string(),
            home_score: Some(*hs),
            away_score: Some(*as_),
            created_at: now,
            updated_at: now,
        };
        insert_match_raw(pool, &m).await?;
    }

    // ── Upcoming EPL matches (scheduled) ─────────────────────────────────
    // Base: 2026-02-25T20:00:00Z onwards
    let upcoming: Vec<(&str, &str, &str, &str)> = vec![
        ("epl_u1",  "epl_1",  "epl_5",  "2026-02-25T20:00:00Z"),
        ("epl_u2",  "epl_4",  "epl_3",  "2026-02-25T17:30:00Z"),
        ("epl_u3",  "epl_2",  "epl_15", "2026-02-26T20:00:00Z"),
        ("epl_u4",  "epl_8",  "epl_11", "2026-02-26T20:00:00Z"),
        ("epl_u5",  "epl_6",  "epl_7",  "2026-02-28T15:00:00Z"),
        ("epl_u6",  "epl_9",  "epl_14", "2026-02-28T17:30:00Z"),
        ("epl_u7",  "epl_10", "epl_17", "2026-03-01T15:00:00Z"),
        ("epl_u8",  "epl_13", "epl_12", "2026-03-01T15:00:00Z"),
        ("epl_u9",  "epl_1",  "epl_2",  "2026-03-04T20:00:00Z"),
        ("epl_u10", "epl_3",  "epl_4",  "2026-03-04T20:00:00Z"),
        ("epl_u11", "epl_7",  "epl_6",  "2026-03-07T15:00:00Z"),
        ("epl_u12", "epl_16", "epl_18", "2026-03-07T17:30:00Z"),
        ("epl_u13", "epl_5",  "epl_3",  "2026-03-14T15:00:00Z"),
        ("epl_u14", "epl_2",  "epl_1",  "2026-03-22T15:30:00Z"),
        ("epl_u15", "epl_3",  "epl_2",  "2026-04-05T15:00:00Z"),
    ];

    for (mid, hid, aid, date_str) in &upcoming {
        let match_date = chrono::DateTime::parse_from_rfc3339(date_str)
            .unwrap()
            .with_timezone(&Utc);
        let m = Match {
            id: mid.to_string(),
            home_team_id: hid.to_string(),
            away_team_id: aid.to_string(),
            home_team_name: name_map[hid].to_string(),
            away_team_name: name_map[aid].to_string(),
            sport: "football".to_string(),
            league: "EPL".to_string(),
            match_date,
            status: "scheduled".to_string(),
            home_score: None,
            away_score: None,
            created_at: now,
            updated_at: now,
        };
        insert_match_raw(pool, &m).await?;

        // Generate prediction
        let home_elo = elo_map[hid];
        let away_elo = elo_map[aid];
        let (hw, aw, dw) = epl_probs(home_elo, away_elo);
        let conf = confidence(home_elo - away_elo);
        let pred = Prediction {
            id: Uuid::new_v4().to_string(),
            match_id: mid.to_string(),
            home_win_probability: hw,
            away_win_probability: aw,
            draw_probability: Some(dw),
            model_version: "ensemble_v1.0".to_string(),
            confidence_score: conf,
            created_at: now,
        };
        insert_prediction_raw(pool, &pred).await?;
    }

    // ── ELO history for top 8 EPL teams ──────────────────────────────────
    let top_epl = [
        ("epl_1",  1490.0_f64, &[1350.0, 1390.0, 1420.0, 1455.0, 1475.0, 1490.0][..]),
        ("epl_2",  1480.0_f64, &[1360.0, 1395.0, 1425.0, 1450.0, 1465.0, 1480.0][..]),
        ("epl_3",  1510.0_f64, &[1430.0, 1455.0, 1470.0, 1488.0, 1502.0, 1510.0][..]),
        ("epl_4",  1380.0_f64, &[1300.0, 1320.0, 1340.0, 1355.0, 1368.0, 1380.0][..]),
        ("epl_5",  1370.0_f64, &[1280.0, 1300.0, 1320.0, 1340.0, 1358.0, 1370.0][..]),
        ("epl_6",  1360.0_f64, &[1310.0, 1325.0, 1338.0, 1348.0, 1355.0, 1360.0][..]),
        ("epl_7",  1340.0_f64, &[1270.0, 1290.0, 1308.0, 1320.0, 1332.0, 1340.0][..]),
        ("epl_8",  1330.0_f64, &[1400.0, 1385.0, 1370.0, 1355.0, 1340.0, 1330.0][..]),
    ];

    let history_dates = [
        "2025-08-01T00:00:00Z",
        "2025-10-01T00:00:00Z",
        "2025-11-15T00:00:00Z",
        "2025-12-20T00:00:00Z",
        "2026-01-15T00:00:00Z",
        "2026-02-20T00:00:00Z",
    ];

    for (team_id, _, ratings) in &top_epl {
        for (i, rating) in ratings.iter().enumerate() {
            let date = chrono::DateTime::parse_from_rfc3339(history_dates[i])
                .unwrap()
                .with_timezone(&Utc);
            sqlx::query(
                "INSERT OR REPLACE INTO elo_history (id, team_id, date, elo_rating, match_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(*team_id)
            .bind(date.to_rfc3339())
            .bind(*rating)
            .bind(Option::<String>::None)
            .execute(pool)
            .await?;
        }
    }

    tracing::info!("EPL data seeded: 20 teams, 35 matches, 15 predictions");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  NBA
// ─────────────────────────────────────────────────────────────────────────────

async fn seed_nba(pool: &SqlitePool) -> Result<()> {
    let now = Utc::now();

    // (id, name, elo, wins, losses, pts_for, pts_against, form)
    let teams: Vec<(&str, &str, f64, i32, i32, i32, i32, &str)> = vec![
        ("nba_1",  "Boston Celtics",            1540.0, 43, 13, 4945, 4580, "WWWWW"),
        ("nba_2",  "Oklahoma City Thunder",     1510.0, 41, 15, 4810, 4590, "WWLWW"),
        ("nba_3",  "Cleveland Cavaliers",       1490.0, 40, 16, 4720, 4540, "WWWLW"),
        ("nba_4",  "Denver Nuggets",            1460.0, 35, 21, 4690, 4620, "WDWLW"),
        ("nba_5",  "New York Knicks",           1430.0, 33, 23, 4620, 4580, "WLWWL"),
        ("nba_6",  "LA Clippers",               1410.0, 31, 25, 4560, 4530, "LWWDW"),
        ("nba_7",  "Minnesota Timberwolves",    1400.0, 30, 26, 4520, 4510, "WLWLD"),
        ("nba_8",  "Dallas Mavericks",          1390.0, 29, 27, 4500, 4490, "DWLWL"),
        ("nba_9",  "Golden State Warriors",     1380.0, 28, 28, 4480, 4480, "LLWWW"),
        ("nba_10", "Phoenix Suns",              1360.0, 26, 30, 4450, 4510, "LWLWL"),
        ("nba_11", "Milwaukee Bucks",           1350.0, 25, 31, 4430, 4490, "WLLWL"),
        ("nba_12", "Miami Heat",                1340.0, 24, 32, 4410, 4470, "LLWLW"),
        ("nba_13", "Sacramento Kings",          1330.0, 23, 33, 4400, 4480, "LLWLL"),
        ("nba_14", "Indiana Pacers",            1320.0, 27, 29, 4470, 4480, "WLWWL"),
        ("nba_15", "Orlando Magic",             1310.0, 26, 30, 4400, 4440, "LWLWW"),
        ("nba_16", "New Orleans Pelicans",      1300.0, 22, 34, 4370, 4490, "LLLWL"),
        ("nba_17", "Atlanta Hawks",             1290.0, 21, 35, 4340, 4510, "LWLLL"),
        ("nba_18", "Brooklyn Nets",             1230.0, 14, 42, 4250, 4590, "LLLLL"),
        ("nba_19", "LA Lakers",                 1370.0, 28, 28, 4470, 4470, "WLWLW"),
        ("nba_20", "Chicago Bulls",             1260.0, 19, 37, 4300, 4520, "LLLWL"),
        ("nba_21", "Utah Jazz",                 1250.0, 16, 40, 4260, 4570, "LLLLL"),
        ("nba_22", "Toronto Raptors",           1240.0, 15, 41, 4240, 4600, "WLLLL"),
        ("nba_23", "Houston Rockets",           1270.0, 28, 28, 4460, 4450, "WWLWW"),
        ("nba_24", "Memphis Grizzlies",         1220.0, 15, 41, 4230, 4590, "LLLWL"),
        ("nba_25", "Portland Trail Blazers",    1210.0, 13, 43, 4210, 4620, "LLLLL"),
        ("nba_26", "San Antonio Spurs",         1200.0, 14, 42, 4200, 4610, "LWLLL"),
        ("nba_27", "Detroit Pistons",           1190.0, 17, 39, 4270, 4540, "LLLWL"),
        ("nba_28", "Charlotte Hornets",         1180.0, 13, 43, 4180, 4610, "LLLLL"),
        ("nba_29", "Washington Wizards",        1170.0, 11, 45, 4150, 4650, "LLLLL"),
        ("nba_30", "Philadelphia 76ers",        1320.0, 22, 34, 4360, 4480, "LWLWL"),
    ];

    for (id, name, elo, w, l, pf, pa, form) in &teams {
        let team = Team {
            id: id.to_string(),
            name: name.to_string(),
            sport: "basketball".to_string(),
            league: "NBA".to_string(),
            logo_url: None,
            elo_rating: *elo,
            created_at: now,
            updated_at: now,
        };
        insert_team_raw(pool, &team).await?;

        let stats = TeamStats {
            id: Uuid::new_v4().to_string(),
            team_id: id.to_string(),
            season: "2025-26".to_string(),
            matches_played: w + l,
            wins: *w,
            draws: None,
            losses: *l,
            goals_for: None,
            goals_against: None,
            points_for: Some(*pf),
            points_against: Some(*pa),
            form: form.to_string(),
            updated_at: now,
        };
        insert_team_stats_raw(pool, &stats).await?;
    }

    let elo_map: std::collections::HashMap<&str, f64> =
        teams.iter().map(|(id, _, elo, ..)| (*id, *elo)).collect();
    let name_map: std::collections::HashMap<&str, &str> =
        teams.iter().map(|(id, name, ..)| (*id, *name)).collect();

    // ── Historical NBA games ──────────────────────────────────────────────
    let historical: Vec<(&str, &str, &str, &str, i32, i32)> = vec![
        ("nba_h1",  "nba_1",  "nba_11", "2025-10-22T01:00:00Z", 115, 108),
        ("nba_h2",  "nba_2",  "nba_9",  "2025-10-24T01:00:00Z", 122, 115),
        ("nba_h3",  "nba_3",  "nba_5",  "2025-11-05T01:00:00Z", 108, 102),
        ("nba_h4",  "nba_4",  "nba_8",  "2025-11-12T01:30:00Z", 118, 112),
        ("nba_h5",  "nba_1",  "nba_19", "2025-11-21T01:00:00Z", 128, 110),
        ("nba_h6",  "nba_2",  "nba_6",  "2025-12-03T01:30:00Z", 115, 109),
        ("nba_h7",  "nba_1",  "nba_9",  "2025-12-25T21:30:00Z", 116, 108),
        ("nba_h8",  "nba_3",  "nba_1",  "2026-01-10T01:00:00Z", 112,  98),
        ("nba_h9",  "nba_2",  "nba_4",  "2026-01-30T01:30:00Z", 108, 100),
        ("nba_h10", "nba_1",  "nba_3",  "2026-02-12T01:00:00Z", 125, 112),
    ];

    for (mid, hid, aid, date_str, hs, as_) in &historical {
        let match_date = chrono::DateTime::parse_from_rfc3339(date_str)
            .unwrap()
            .with_timezone(&Utc);
        let m = Match {
            id: mid.to_string(),
            home_team_id: hid.to_string(),
            away_team_id: aid.to_string(),
            home_team_name: name_map[hid].to_string(),
            away_team_name: name_map[aid].to_string(),
            sport: "basketball".to_string(),
            league: "NBA".to_string(),
            match_date,
            status: "finished".to_string(),
            home_score: Some(*hs),
            away_score: Some(*as_),
            created_at: now,
            updated_at: now,
        };
        insert_match_raw(pool, &m).await?;
    }

    // ── Upcoming NBA games ────────────────────────────────────────────────
    let upcoming: Vec<(&str, &str, &str, &str)> = vec![
        ("nba_u1",  "nba_1",  "nba_11", "2026-02-25T01:00:00Z"),
        ("nba_u2",  "nba_2",  "nba_4",  "2026-02-25T01:30:00Z"),
        ("nba_u3",  "nba_3",  "nba_5",  "2026-02-26T01:00:00Z"),
        ("nba_u4",  "nba_19", "nba_9",  "2026-02-26T01:30:00Z"),
        ("nba_u5",  "nba_12", "nba_14", "2026-02-27T01:00:00Z"),
        ("nba_u6",  "nba_8",  "nba_10", "2026-02-27T01:30:00Z"),
        ("nba_u7",  "nba_1",  "nba_3",  "2026-02-28T01:00:00Z"),
        ("nba_u8",  "nba_4",  "nba_2",  "2026-03-01T01:30:00Z"),
        ("nba_u9",  "nba_5",  "nba_1",  "2026-03-02T01:00:00Z"),
        ("nba_u10", "nba_9",  "nba_6",  "2026-03-03T01:30:00Z"),
        ("nba_u11", "nba_11", "nba_20", "2026-03-04T01:00:00Z"),
        ("nba_u12", "nba_14", "nba_12", "2026-03-05T01:30:00Z"),
        ("nba_u13", "nba_10", "nba_13", "2026-03-06T01:00:00Z"),
        ("nba_u14", "nba_3",  "nba_19", "2026-03-07T01:30:00Z"),
        ("nba_u15", "nba_2",  "nba_1",  "2026-03-08T01:00:00Z"),
    ];

    for (mid, hid, aid, date_str) in &upcoming {
        let match_date = chrono::DateTime::parse_from_rfc3339(date_str)
            .unwrap()
            .with_timezone(&Utc);
        let m = Match {
            id: mid.to_string(),
            home_team_id: hid.to_string(),
            away_team_id: aid.to_string(),
            home_team_name: name_map[hid].to_string(),
            away_team_name: name_map[aid].to_string(),
            sport: "basketball".to_string(),
            league: "NBA".to_string(),
            match_date,
            status: "scheduled".to_string(),
            home_score: None,
            away_score: None,
            created_at: now,
            updated_at: now,
        };
        insert_match_raw(pool, &m).await?;

        let home_elo = elo_map[hid];
        let away_elo = elo_map[aid];
        let (hw, aw) = nba_probs(home_elo, away_elo);
        let conf = confidence(home_elo - away_elo);
        let pred = Prediction {
            id: Uuid::new_v4().to_string(),
            match_id: mid.to_string(),
            home_win_probability: hw,
            away_win_probability: aw,
            draw_probability: None,
            model_version: "ensemble_v1.0".to_string(),
            confidence_score: conf,
            created_at: now,
        };
        insert_prediction_raw(pool, &pred).await?;
    }

    // ── ELO history for top 6 NBA teams ──────────────────────────────────
    let top_nba = [
        ("nba_1",  &[1450.0, 1480.0, 1505.0, 1520.0, 1532.0, 1540.0][..]),
        ("nba_2",  &[1420.0, 1450.0, 1470.0, 1488.0, 1500.0, 1510.0][..]),
        ("nba_3",  &[1400.0, 1430.0, 1452.0, 1468.0, 1480.0, 1490.0][..]),
        ("nba_4",  &[1410.0, 1430.0, 1442.0, 1450.0, 1456.0, 1460.0][..]),
        ("nba_19", &[1330.0, 1345.0, 1355.0, 1362.0, 1367.0, 1370.0][..]),
        ("nba_11", &[1380.0, 1370.0, 1362.0, 1357.0, 1353.0, 1350.0][..]),
    ];

    let nba_dates = [
        "2025-10-01T00:00:00Z",
        "2025-11-01T00:00:00Z",
        "2025-12-01T00:00:00Z",
        "2026-01-01T00:00:00Z",
        "2026-01-15T00:00:00Z",
        "2026-02-20T00:00:00Z",
    ];

    for (team_id, ratings) in &top_nba {
        for (i, rating) in ratings.iter().enumerate() {
            let date = chrono::DateTime::parse_from_rfc3339(nba_dates[i])
                .unwrap()
                .with_timezone(&Utc);
            sqlx::query(
                "INSERT OR REPLACE INTO elo_history (id, team_id, date, elo_rating, match_id) VALUES (?,?,?,?,?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(*team_id)
            .bind(date.to_rfc3339())
            .bind(*rating)
            .bind(Option::<String>::None)
            .execute(pool)
            .await?;
        }
    }

    tracing::info!("NBA data seeded: 30 teams, 25 matches, 15 predictions");
    Ok(())
}
