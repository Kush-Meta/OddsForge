use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::env;

use crate::db::{insert_match, insert_team};
use crate::models::{Match, Team};

// Football Data API structures
#[derive(Debug, Deserialize)]
pub struct FootballDataCompetitions {
    pub competitions: Vec<Competition>,
}

#[derive(Debug, Deserialize)]
pub struct Competition {
    pub id: u32,
    pub name: String,
    pub code: String,
    pub area: Area,
}

#[derive(Debug, Deserialize)]
pub struct Area {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct FootballDataTeams {
    pub teams: Vec<FootballTeam>,
}

#[derive(Debug, Deserialize)]
pub struct FootballTeam {
    pub id: u32,
    pub name: String,
    pub crest: Option<String>,
    pub area: Area,
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

// NBA API structures (using balldontlie.io)
#[derive(Debug, Deserialize)]
pub struct NbaTeamsResponse {
    pub data: Vec<NbaTeam>,
}

#[derive(Debug, Deserialize)]
pub struct NbaTeam {
    pub id: u32,
    pub abbreviation: String,
    pub city: String,
    pub conference: String,
    pub division: String,
    pub full_name: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct NbaGamesResponse {
    pub data: Vec<NbaGame>,
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

pub struct DataFetcher {
    client: Client,
    football_api_key: Option<String>,
}

impl DataFetcher {
    pub fn new() -> Self {
        let client = Client::new();
        let football_api_key = env::var("FOOTBALL_DATA_API_KEY").ok();
        
        Self {
            client,
            football_api_key,
        }
    }

    pub async fn fetch_epl_teams(&self, pool: &SqlitePool) -> Result<()> {
        tracing::info!("Fetching EPL teams from football-data.org");
        
        let api_key = self.football_api_key.as_ref()
            .ok_or_else(|| anyhow!("FOOTBALL_DATA_API_KEY not set"))?;

        // EPL competition ID is 2021
        let url = "https://api.football-data.org/v4/competitions/2021/teams";
        
        let response = self.client
            .get(url)
            .header("X-Auth-Token", api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("API request failed: {}", response.status()));
        }

        let teams_data: FootballDataTeams = response.json().await?;
        
        for team_data in teams_data.teams {
            let team = Team {
                id: format!("epl_{}", team_data.id),
                name: team_data.name,
                sport: "football".to_string(),
                league: "EPL".to_string(),
                logo_url: team_data.crest,
                elo_rating: 1200.0, // Default ELO rating
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            insert_team(pool, &team).await?;
            tracing::debug!("Inserted team: {}", team.name);
        }

        tracing::info!("Successfully fetched and stored EPL teams");
        Ok(())
    }

    pub async fn fetch_champions_league_teams(&self, pool: &SqlitePool) -> Result<()> {
        tracing::info!("Fetching Champions League teams from football-data.org");
        
        let api_key = self.football_api_key.as_ref()
            .ok_or_else(|| anyhow!("FOOTBALL_DATA_API_KEY not set"))?;

        // Champions League competition ID is 2001
        let url = "https://api.football-data.org/v4/competitions/2001/teams";
        
        let response = self.client
            .get(url)
            .header("X-Auth-Token", api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("API request failed: {}", response.status()));
        }

        let teams_data: FootballDataTeams = response.json().await?;
        
        for team_data in teams_data.teams {
            let team = Team {
                id: format!("ucl_{}", team_data.id),
                name: team_data.name,
                sport: "football".to_string(),
                league: "Champions League".to_string(),
                logo_url: team_data.crest,
                elo_rating: 1200.0, // Default ELO rating
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            insert_team(pool, &team).await?;
            tracing::debug!("Inserted team: {}", team.name);
        }

        tracing::info!("Successfully fetched and stored Champions League teams");
        Ok(())
    }

    pub async fn fetch_epl_matches(&self, pool: &SqlitePool) -> Result<()> {
        tracing::info!("Fetching EPL matches from football-data.org");
        
        let api_key = self.football_api_key.as_ref()
            .ok_or_else(|| anyhow!("FOOTBALL_DATA_API_KEY not set"))?;

        // Fetch upcoming matches
        let url = "https://api.football-data.org/v4/competitions/2021/matches?status=SCHEDULED";
        
        let response = self.client
            .get(url)
            .header("X-Auth-Token", api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("API request failed: {}", response.status()));
        }

        let matches_data: FootballDataMatches = response.json().await?;
        
        for match_data in matches_data.matches {
            let match_date = DateTime::parse_from_rfc3339(&match_data.utc_date)?
                .with_timezone(&Utc);

            let match_obj = Match {
                id: format!("epl_{}", match_data.id),
                home_team_id: format!("epl_{}", match_data.home_team.id),
                away_team_id: format!("epl_{}", match_data.away_team.id),
                home_team_name: match_data.home_team.name,
                away_team_name: match_data.away_team.name,
                sport: "football".to_string(),
                league: "EPL".to_string(),
                match_date,
                status: match_data.status.to_lowercase(),
                home_score: match_data.score.full_time.as_ref().and_then(|s| s.home.map(|h| h as i32)),
                away_score: match_data.score.full_time.as_ref().and_then(|s| s.away.map(|a| a as i32)),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            insert_match(pool, &match_obj).await?;
            tracing::debug!("Inserted match: {} vs {}", match_obj.home_team_name, match_obj.away_team_name);
        }

        tracing::info!("Successfully fetched and stored EPL matches");
        Ok(())
    }

    pub async fn fetch_nba_teams(&self, pool: &SqlitePool) -> Result<()> {
        tracing::info!("Fetching NBA teams from balldontlie.io");
        
        let url = "https://api.balldontlie.io/v1/teams";
        
        let response = self.client
            .get(url)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("API request failed: {}", response.status()));
        }

        let teams_data: NbaTeamsResponse = response.json().await?;
        
        for team_data in teams_data.data {
            let team = Team {
                id: format!("nba_{}", team_data.id),
                name: team_data.full_name,
                sport: "basketball".to_string(),
                league: "NBA".to_string(),
                logo_url: None, // balldontlie.io doesn't provide logo URLs
                elo_rating: 1200.0, // Default ELO rating
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            insert_team(pool, &team).await?;
            tracing::debug!("Inserted team: {}", team.name);
        }

        tracing::info!("Successfully fetched and stored NBA teams");
        Ok(())
    }

    pub async fn fetch_nba_games(&self, pool: &SqlitePool) -> Result<()> {
        tracing::info!("Fetching NBA games from balldontlie.io");
        
        let url = "https://api.balldontlie.io/v1/games?seasons[]=2024&per_page=100";
        
        let response = self.client
            .get(url)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("API request failed: {}", response.status()));
        }

        let games_data: NbaGamesResponse = response.json().await?;
        
        for game_data in games_data.data {
            let match_date = DateTime::parse_from_rfc3339(&format!("{}T00:00:00Z", game_data.date))?
                .with_timezone(&Utc);

            let status = if game_data.home_team_score.is_some() && game_data.visitor_team_score.is_some() {
                "finished".to_string()
            } else {
                "scheduled".to_string()
            };

            let match_obj = Match {
                id: format!("nba_{}", game_data.id),
                home_team_id: format!("nba_{}", game_data.home_team.id),
                away_team_id: format!("nba_{}", game_data.visitor_team.id),
                home_team_name: game_data.home_team.full_name,
                away_team_name: game_data.visitor_team.full_name,
                sport: "basketball".to_string(),
                league: "NBA".to_string(),
                match_date,
                status,
                home_score: game_data.home_team_score.map(|s| s as i32),
                away_score: game_data.visitor_team_score.map(|s| s as i32),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            insert_match(pool, &match_obj).await?;
            tracing::debug!("Inserted match: {} vs {}", match_obj.home_team_name, match_obj.away_team_name);
        }

        tracing::info!("Successfully fetched and stored NBA games");
        Ok(())
    }

    pub async fn fetch_all_data(&self, pool: &SqlitePool) -> Result<()> {
        tracing::info!("Fetching all sports data...");
        
        // Fetch football data if API key is available
        if self.football_api_key.is_some() {
            self.fetch_epl_teams(pool).await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Rate limiting
            
            self.fetch_champions_league_teams(pool).await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            
            self.fetch_epl_matches(pool).await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        } else {
            tracing::warn!("Football Data API key not found, skipping football data");
        }
        
        // Fetch NBA data (free API)
        self.fetch_nba_teams(pool).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        
        self.fetch_nba_games(pool).await?;
        
        tracing::info!("All data fetching completed");
        Ok(())
    }
}