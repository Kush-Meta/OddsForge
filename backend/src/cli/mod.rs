use anyhow::Result;
use sqlx::Row;

use crate::db::{create_pool, get_upcoming_matches};
use crate::services::{DataFetcher, PredictionEngine};

pub async fn fetch_data(sport: &str) -> Result<()> {
    let pool = create_pool().await?;
    let fetcher = DataFetcher::new();

    println!("🏈 Fetching {} data...", sport);

    match sport.to_lowercase().as_str() {
        "football" | "soccer" => {
            println!("📥 Fetching EPL teams...");
            fetcher.fetch_epl_teams(&pool).await?;
            
            println!("📥 Fetching Champions League teams...");
            fetcher.fetch_champions_league_teams(&pool).await?;
            
            println!("📥 Fetching EPL matches...");
            fetcher.fetch_epl_matches(&pool).await?;
            
            println!("✅ Football data fetched successfully!");
        }
        "basketball" | "nba" => {
            println!("📥 Fetching NBA teams...");
            fetcher.fetch_nba_teams(&pool).await?;
            
            println!("📥 Fetching NBA games...");
            fetcher.fetch_nba_games(&pool).await?;
            
            println!("✅ Basketball data fetched successfully!");
        }
        "all" => {
            println!("📥 Fetching all sports data...");
            fetcher.fetch_all_data(&pool).await?;
            println!("✅ All sports data fetched successfully!");
        }
        _ => {
            println!("❌ Unsupported sport: {}. Use 'football', 'basketball', or 'all'", sport);
            return Ok(());
        }
    }

    Ok(())
}

pub async fn generate_predictions() -> Result<()> {
    let pool = create_pool().await?;
    let prediction_engine = PredictionEngine::new();

    println!("🔮 Generating predictions for upcoming matches...");

    let matches = get_upcoming_matches(&pool, None).await?;
    
    if matches.is_empty() {
        println!("📭 No upcoming matches found. Try fetching data first with: oddsforge fetch --sport all");
        return Ok(());
    }

    prediction_engine.generate_predictions(&pool, &matches).await?;
    
    println!("✅ Generated predictions for {} matches!", matches.len());
    
    // Display a few examples
    println!("\n🎯 Sample predictions:");
    for (i, match_data) in matches.iter().take(5).enumerate() {
        if let Ok(Some(prediction)) = crate::db::get_prediction_by_match_id(&pool, &match_data.id).await {
            println!("{}. {} vs {} ({}):",
                i + 1,
                match_data.home_team_name,
                match_data.away_team_name,
                match_data.match_date.format("%Y-%m-%d %H:%M")
            );
            println!("   Home win: {:.1}% | Away win: {:.1}%{}",
                prediction.home_win_probability * 100.0,
                prediction.away_win_probability * 100.0,
                prediction.draw_probability.map_or(String::new(), |d| format!(" | Draw: {:.1}%", d * 100.0))
            );
            println!("   Confidence: {:.1}%\n", prediction.confidence_score * 100.0);
        }
    }

    Ok(())
}

pub async fn query_team(team_name: &str) -> Result<()> {
    let pool = create_pool().await?;

    println!("🔍 Searching for team: {}", team_name);

    // First try to find the team by name (case-insensitive search)
    let teams = sqlx::query_as::<_, crate::models::Team>(
        "SELECT * FROM teams WHERE LOWER(name) LIKE LOWER(?) ORDER BY name"
    )
    .bind(format!("%{}%", team_name))
    .fetch_all(&pool)
    .await?;

    if teams.is_empty() {
        println!("❌ No teams found matching '{}'", team_name);
        
        // Show available teams for suggestions
        println!("\n💡 Available teams:");
        let all_teams = sqlx::query_as::<_, crate::models::Team>(
            "SELECT * FROM teams ORDER BY league, name LIMIT 10"
        )
        .fetch_all(&pool)
        .await?;

        for team in all_teams {
            println!("   • {} ({})", team.name, team.league);
        }
        
        return Ok(());
    }

    if teams.len() > 1 {
        println!("📋 Found {} teams matching '{}':\n", teams.len(), team_name);
        for (i, team) in teams.iter().enumerate() {
            println!("{}. {} ({} - {})", i + 1, team.name, team.league, team.sport);
        }
        println!("\n🔍 Showing details for first match:");
    }

    let team = &teams[0];
    
    println!("📊 Team Details:");
    println!("   Name: {}", team.name);
    println!("   League: {} ({})", team.league, team.sport);
    println!("   ELO Rating: {:.1}", team.elo_rating);
    println!("   Last Updated: {}", team.updated_at.format("%Y-%m-%d %H:%M:%S"));

    // Get recent matches
    println!("\n📅 Recent Matches:");
    let recent_matches = sqlx::query_as::<_, crate::models::Match>(
        r#"
        SELECT * FROM matches 
        WHERE (home_team_id = ? OR away_team_id = ?) 
            AND status = 'finished'
        ORDER BY match_date DESC 
        LIMIT 5
        "#
    )
    .bind(&team.id)
    .bind(&team.id)
    .fetch_all(&pool)
    .await?;

    if recent_matches.is_empty() {
        println!("   No recent matches found");
    } else {
        for match_data in recent_matches {
            let (opponent, is_home, result) = if match_data.home_team_id == team.id {
                let result = match (match_data.home_score, match_data.away_score) {
                    (Some(h), Some(a)) => {
                        if h > a { "W" } else if h < a { "L" } else { "D" }
                    }
                    _ => "?"
                };
                (match_data.away_team_name, true, result)
            } else {
                let result = match (match_data.home_score, match_data.away_score) {
                    (Some(h), Some(a)) => {
                        if a > h { "W" } else if a < h { "L" } else { "D" }
                    }
                    _ => "?"
                };
                (match_data.home_team_name, false, result)
            };

            let venue = if is_home { "vs" } else { "at" };
            let score = match (match_data.home_score, match_data.away_score) {
                (Some(h), Some(a)) => format!("({}-{})", h, a),
                _ => "(TBD)".to_string(),
            };

            println!("   {} {} {} {} {}", 
                match_data.match_date.format("%m/%d"), 
                venue, 
                opponent, 
                score,
                result
            );
        }
    }

    // Get upcoming matches
    println!("\n📅 Upcoming Matches:");
    let upcoming_matches = sqlx::query_as::<_, crate::models::Match>(
        r#"
        SELECT * FROM matches 
        WHERE (home_team_id = ? OR away_team_id = ?) 
            AND status = 'scheduled'
            AND match_date > datetime('now')
        ORDER BY match_date ASC 
        LIMIT 5
        "#
    )
    .bind(&team.id)
    .bind(&team.id)
    .fetch_all(&pool)
    .await?;

    if upcoming_matches.is_empty() {
        println!("   No upcoming matches found");
    } else {
        for match_data in upcoming_matches {
            let (opponent, is_home) = if match_data.home_team_id == team.id {
                (match_data.away_team_name, true)
            } else {
                (match_data.home_team_name, false)
            };

            let venue = if is_home { "vs" } else { "at" };

            // Try to get prediction
            if let Ok(Some(prediction)) = crate::db::get_prediction_by_match_id(&pool, &match_data.id).await {
                let team_win_prob = if is_home {
                    prediction.home_win_probability
                } else {
                    prediction.away_win_probability
                };

                println!("   {} {} {} - Win probability: {:.1}%", 
                    match_data.match_date.format("%m/%d %H:%M"), 
                    venue, 
                    opponent,
                    team_win_prob * 100.0
                );
            } else {
                println!("   {} {} {}", 
                    match_data.match_date.format("%m/%d %H:%M"), 
                    venue, 
                    opponent
                );
            }
        }
    }

    Ok(())
}

pub async fn show_leagues() -> Result<()> {
    let pool = create_pool().await?;

    println!("🏆 Available Leagues:\n");

    let leagues = sqlx::query(
        "SELECT sport, league, COUNT(*) as team_count FROM teams GROUP BY sport, league ORDER BY sport, league"
    )
    .fetch_all(&pool)
    .await?;

    let mut current_sport = String::new();
    for row in leagues {
        let sport: String = row.get("sport");
        let league: String = row.get("league");
        let count: i64 = row.get("team_count");

        if sport != current_sport {
            if !current_sport.is_empty() {
                println!();
            }
            println!("📊 {}:", sport.to_uppercase());
            current_sport = sport;
        }

        println!("   • {} ({} teams)", league, count);
    }

    println!("\n💡 Use 'oddsforge team --name <team_name>' to get team details");
    println!("💡 Use 'oddsforge fetch --sport <sport>' to update data");

    Ok(())
}

pub async fn show_edges() -> Result<()> {
    let pool = create_pool().await?;
    let prediction_engine = PredictionEngine::new();

    println!("🎯 Finding market edges...\n");

    let edges = prediction_engine.find_market_edges(&pool).await?;

    if edges.is_empty() {
        println!("📭 No significant edges found at the moment.");
        println!("💡 Try running predictions first: oddsforge predict");
        return Ok(());
    }

    println!("💰 Top Market Edges:\n");
    
    for (i, edge) in edges.iter().take(10).enumerate() {
        println!("{}. {} vs {} ({}):",
            i + 1,
            edge.match_info.home_team_name,
            edge.match_info.away_team_name,
            edge.match_info.match_date.format("%Y-%m-%d %H:%M")
        );
        
        println!("   Our model: Home {:.1}% | Away {:.1}%{}",
            edge.our_prediction.home_win_probability * 100.0,
            edge.our_prediction.away_win_probability * 100.0,
            edge.our_prediction.draw_probability.map_or(String::new(), |d| format!(" | Draw {:.1}%", d * 100.0))
        );
        
        println!("   Market odds: {:.2} | {:.2}{}",
            edge.market_home_odds,
            edge.market_away_odds,
            edge.market_draw_odds.map_or(String::new(), |d| format!(" | {:.2}", d))
        );
        
        println!("   Edge value: {:.1}%", edge.edge_value * 100.0);
        println!("   Confidence: {:.1}%\n", edge.our_prediction.confidence_score * 100.0);
    }

    println!("⚠️  Note: Market odds are simulated for demonstration purposes.");
    println!("💡 In production, these would be fetched from betting APIs.");

    Ok(())
}