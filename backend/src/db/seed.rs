use crate::models::*;
use sqlx::SqlitePool;
use chrono::{Utc, Duration};
use uuid::Uuid;
use rand;

pub async fn seed_database(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    println!("🌱 Seeding database with sample data...");
    
    // Create tables first
    create_tables(pool).await?;
    
    // Seed teams
    seed_teams(pool).await?;
    
    // Seed historical matches
    seed_historical_matches(pool).await?;
    
    // Seed upcoming matches
    seed_upcoming_matches(pool).await?;
    
    // Seed ELO history
    seed_elo_history(pool).await?;
    
    println!("✅ Database seeded successfully!");
    Ok(())
}

async fn create_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS teams (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            sport TEXT NOT NULL,
            league TEXT NOT NULL,
            elo_rating REAL NOT NULL DEFAULT 1200.0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS matches (
            id TEXT PRIMARY KEY,
            home_team_id TEXT NOT NULL,
            away_team_id TEXT NOT NULL,
            sport TEXT NOT NULL,
            league TEXT NOT NULL,
            match_date DATETIME NOT NULL,
            home_score INTEGER,
            away_score INTEGER,
            status TEXT NOT NULL DEFAULT 'scheduled',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (home_team_id) REFERENCES teams (id),
            FOREIGN KEY (away_team_id) REFERENCES teams (id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS predictions (
            id TEXT PRIMARY KEY,
            match_id TEXT NOT NULL,
            home_win_prob REAL NOT NULL,
            draw_prob REAL,
            away_win_prob REAL NOT NULL,
            confidence REAL NOT NULL,
            model_version TEXT NOT NULL DEFAULT 'v1.0',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (match_id) REFERENCES matches (id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS market_odds (
            id TEXT PRIMARY KEY,
            match_id TEXT NOT NULL,
            home_odds REAL NOT NULL,
            draw_odds REAL,
            away_odds REAL NOT NULL,
            source TEXT NOT NULL DEFAULT 'sample',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (match_id) REFERENCES matches (id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS elo_history (
            id TEXT PRIMARY KEY,
            team_id TEXT NOT NULL,
            elo_rating REAL NOT NULL,
            date DATETIME NOT NULL,
            match_id TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (team_id) REFERENCES teams (id),
            FOREIGN KEY (match_id) REFERENCES matches (id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn seed_teams(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // EPL Teams
    let epl_teams = vec![
        ("epl_1", "Arsenal", 1450.0),
        ("epl_2", "Manchester City", 1520.0),
        ("epl_3", "Liverpool", 1480.0),
        ("epl_4", "Chelsea", 1420.0),
        ("epl_5", "Manchester United", 1400.0),
        ("epl_6", "Tottenham", 1380.0),
        ("epl_7", "Newcastle", 1350.0),
        ("epl_8", "Brighton", 1320.0),
        ("epl_9", "Aston Villa", 1340.0),
        ("epl_10", "West Ham", 1290.0),
        ("epl_11", "Crystal Palace", 1260.0),
        ("epl_12", "Fulham", 1280.0),
        ("epl_13", "Brentford", 1270.0),
        ("epl_14", "Wolves", 1240.0),
        ("epl_15", "Everton", 1220.0),
        ("epl_16", "Nottingham Forest", 1210.0),
        ("epl_17", "Bournemouth", 1200.0),
        ("epl_18", "Sheffield United", 1180.0),
        ("epl_19", "Burnley", 1170.0),
        ("epl_20", "Luton Town", 1160.0),
    ];

    for (id, name, elo) in epl_teams {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO teams (id, name, sport, league, elo_rating)
            VALUES (?, ?, 'football', 'EPL', ?)
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(elo)
        .execute(pool)
        .await?;
    }

    // NBA Teams
    let nba_teams = vec![
        ("nba_1", "Boston Celtics", 1540.0),
        ("nba_2", "Denver Nuggets", 1520.0),
        ("nba_3", "Phoenix Suns", 1490.0),
        ("nba_4", "Milwaukee Bucks", 1480.0),
        ("nba_5", "Philadelphia 76ers", 1450.0),
        ("nba_6", "Miami Heat", 1430.0),
        ("nba_7", "Golden State Warriors", 1420.0),
        ("nba_8", "Los Angeles Lakers", 1410.0),
        ("nba_9", "Dallas Mavericks", 1380.0),
        ("nba_10", "Sacramento Kings", 1360.0),
        ("nba_11", "New York Knicks", 1340.0),
        ("nba_12", "Brooklyn Nets", 1320.0),
        ("nba_13", "Atlanta Hawks", 1300.0),
        ("nba_14", "Chicago Bulls", 1280.0),
        ("nba_15", "Los Angeles Clippers", 1400.0),
        ("nba_16", "Toronto Raptors", 1260.0),
        ("nba_17", "Indiana Pacers", 1290.0),
        ("nba_18", "Orlando Magic", 1250.0),
        ("nba_19", "Washington Wizards", 1200.0),
        ("nba_20", "Charlotte Hornets", 1190.0),
        ("nba_21", "Minnesota Timberwolves", 1350.0),
        ("nba_22", "New Orleans Pelicans", 1310.0),
        ("nba_23", "Utah Jazz", 1240.0),
        ("nba_24", "Oklahoma City Thunder", 1380.0),
        ("nba_25", "Houston Rockets", 1220.0),
        ("nba_26", "Memphis Grizzlies", 1330.0),
        ("nba_27", "Cleveland Cavaliers", 1370.0),
        ("nba_28", "Detroit Pistons", 1170.0),
        ("nba_29", "San Antonio Spurs", 1180.0),
        ("nba_30", "Portland Trail Blazers", 1210.0),
    ];

    for (id, name, elo) in nba_teams {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO teams (id, name, sport, league, elo_rating)
            VALUES (?, ?, 'basketball', 'NBA', ?)
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(elo)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn seed_historical_matches(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    
    // Sample historical matches with results
    let historical_matches = vec![
        // EPL matches
        ("epl_1", "epl_2", 2, 1, -7), // Arsenal vs Man City
        ("epl_3", "epl_4", 3, 1, -14), // Liverpool vs Chelsea
        ("epl_5", "epl_6", 1, 2, -21), // Man United vs Tottenham
        ("epl_7", "epl_8", 2, 0, -28), // Newcastle vs Brighton
        ("epl_9", "epl_10", 1, 1, -35), // Aston Villa vs West Ham
        // NBA matches
        ("nba_1", "nba_2", 118, 112, -3), // Celtics vs Nuggets
        ("nba_3", "nba_4", 109, 115, -10), // Suns vs Bucks
        ("nba_5", "nba_6", 102, 98, -17), // 76ers vs Heat
        ("nba_7", "nba_8", 124, 120, -24), // Warriors vs Lakers
        ("nba_9", "nba_10", 110, 106, -31), // Mavs vs Kings
    ];

    for (home_id, away_id, home_score, away_score, days_ago) in historical_matches {
        let match_id = Uuid::new_v4().to_string();
        let sport = if home_id.starts_with("epl") { "football" } else { "basketball" };
        let league = if home_id.starts_with("epl") { "EPL" } else { "NBA" };
        let match_date = now + Duration::days(days_ago);

        sqlx::query(
            r#"
            INSERT INTO matches (id, home_team_id, away_team_id, sport, league, match_date, home_score, away_score, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'completed')
            "#,
        )
        .bind(&match_id)
        .bind(home_id)
        .bind(away_id)
        .bind(sport)
        .bind(league)
        .bind(match_date)
        .bind(home_score)
        .bind(away_score)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn seed_upcoming_matches(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    
    // Upcoming matches
    let upcoming_matches = vec![
        // EPL matches
        ("epl_1", "epl_3", 2), // Arsenal vs Liverpool
        ("epl_2", "epl_4", 5), // Man City vs Chelsea
        ("epl_5", "epl_7", 7), // Man United vs Newcastle
        ("epl_6", "epl_8", 10), // Tottenham vs Brighton
        ("epl_9", "epl_11", 14), // Aston Villa vs Crystal Palace
        ("epl_10", "epl_12", 16), // West Ham vs Fulham
        ("epl_13", "epl_14", 19), // Brentford vs Wolves
        ("epl_15", "epl_16", 21), // Everton vs Nottingham Forest
        // NBA matches
        ("nba_1", "nba_3", 1), // Celtics vs Suns
        ("nba_2", "nba_4", 3), // Nuggets vs Bucks
        ("nba_5", "nba_7", 4), // 76ers vs Warriors
        ("nba_6", "nba_8", 6), // Heat vs Lakers
        ("nba_9", "nba_11", 8), // Mavs vs Knicks
        ("nba_10", "nba_12", 11), // Kings vs Nets
        ("nba_13", "nba_15", 13), // Hawks vs Clippers
        ("nba_14", "nba_16", 15), // Bulls vs Raptors
    ];

    for (home_id, away_id, days_ahead) in upcoming_matches {
        let match_id = Uuid::new_v4().to_string();
        let sport = if home_id.starts_with("epl") { "football" } else { "basketball" };
        let league = if home_id.starts_with("epl") { "EPL" } else { "NBA" };
        let match_date = now + Duration::days(days_ahead);

        sqlx::query(
            r#"
            INSERT INTO matches (id, home_team_id, away_team_id, sport, league, match_date, status)
            VALUES (?, ?, ?, ?, ?, ?, 'scheduled')
            "#,
        )
        .bind(&match_id)
        .bind(home_id)
        .bind(away_id)
        .bind(sport)
        .bind(league)
        .bind(match_date)
        .execute(pool)
        .await?;

        // Generate predictions for each match
        let home_win_prob = 0.4 + (rand::random::<f64>() * 0.4);
        let away_win_prob = if sport == "football" {
            0.6 - home_win_prob - 0.25 // Reserve 0.25 for draw
        } else {
            1.0 - home_win_prob
        };
        let draw_prob = if sport == "football" { Some(0.25) } else { None };
        let confidence = 0.6 + (rand::random::<f64>() * 0.3);

        let prediction_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO predictions (id, match_id, home_win_prob, draw_prob, away_win_prob, confidence)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&prediction_id)
        .bind(&match_id)
        .bind(home_win_prob)
        .bind(draw_prob)
        .bind(away_win_prob)
        .bind(confidence)
        .execute(pool)
        .await?;

        // Generate market odds
        let home_odds = 1.0 / (home_win_prob + 0.05); // Add bookmaker margin
        let away_odds = 1.0 / (away_win_prob + 0.05);
        let draw_odds = if sport == "football" { Some(1.0 / (0.25 + 0.05)) } else { None };

        let odds_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO market_odds (id, match_id, home_odds, draw_odds, away_odds)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&odds_id)
        .bind(&match_id)
        .bind(home_odds)
        .bind(draw_odds)
        .bind(away_odds)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn seed_elo_history(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    
    // Sample ELO progression for top teams
    let top_teams = vec![
        ("epl_1", 1450.0), // Arsenal
        ("epl_2", 1520.0), // Man City
        ("epl_3", 1480.0), // Liverpool
        ("nba_1", 1540.0), // Celtics
        ("nba_2", 1520.0), // Nuggets
        ("nba_3", 1490.0), // Suns
    ];

    for (team_id, current_elo) in top_teams {
        // Generate 6 months of ELO history (weekly points)
        for weeks_ago in (0..26).step_by(1) {
            let elo_variation = (rand::random::<f64>() - 0.5) * 100.0; // ±50 ELO variation
            let historical_elo = current_elo + elo_variation;
            let history_date = now - Duration::weeks(weeks_ago);
            
            let history_id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO elo_history (id, team_id, elo_rating, date)
                VALUES (?, ?, ?, ?)
                "#,
            )
            .bind(&history_id)
            .bind(team_id)
            .bind(historical_elo)
            .bind(history_date)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}