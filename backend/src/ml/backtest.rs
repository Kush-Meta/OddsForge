//! Walk-Forward Backtesting Pipeline
//!
//! Annual folds over historical NBA data.
//! Each fold k: train on years 0..k-1, evaluate on year k.
//! Reports Brier score, log-loss, accuracy per fold.
//! Also runs the full final training on all data.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::models::Match;
use super::{
    feature_store::{build_features, N_FEATURES},
    gradient_boosted::GradientBoostedModel,
    meta_learner::{
        build_meta_features, team_factors_from_features, IsotonicCalibrator, MetaLearner,
        MlModelState, META_FEATURES,
    },
    monte_carlo::monte_carlo_win_prob,
    poisson_model::PoissonModel,
    rapm::RapmModel,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldResult {
    pub fold: i32,
    pub year: i32,
    pub n_games: i32,
    pub brier_score: f64,
    pub log_loss: f64,
    pub accuracy: f64,
}

fn parse_match(row: &sqlx::sqlite::SqliteRow) -> Result<Match> {
    Ok(Match {
        id: row.get("id"),
        home_team_id: row.get("home_team_id"),
        away_team_id: row.get("away_team_id"),
        home_team_name: row.get("home_team_name"),
        away_team_name: row.get("away_team_name"),
        sport: row.get("sport"),
        league: row.get("league"),
        match_date: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("match_date"))?.with_timezone(&chrono::Utc),
        status: row.get("status"),
        home_score: row.get("home_score"),
        away_score: row.get("away_score"),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&chrono::Utc),
    })
}

fn base_probs(
    m: &Match,
    f: &[f64; N_FEATURES],
    poisson: &PoissonModel,
    rapm: &RapmModel,
    gbt: &GradientBoostedModel,
    mc_sims: usize,
) -> [f64; META_FEATURES] {
    let p_po = poisson.predict_home_win_prob(&m.home_team_id, &m.away_team_id);
    let p_ra = rapm.predict_home_win_prob(&m.home_team_id, &m.away_team_id);
    let p_gb = gbt.predict_proba(f);
    let hf = team_factors_from_features(f, true);
    let af = team_factors_from_features(f, false);
    let (p_mc, _) = monte_carlo_win_prob(&hf, &af, mc_sims);
    build_meta_features(p_po, p_ra, p_gb, p_mc, f)
}

fn eval_fold(
    eval_games: &[(Match, [f64; N_FEATURES], bool)],
    poisson: &PoissonModel,
    rapm: &RapmModel,
    gbt: &GradientBoostedModel,
    meta: &MetaLearner,
) -> (f64, f64, f64, Vec<(f64, f64)>) {
    let mut brier = 0.0f64;
    let mut ll = 0.0f64;
    let mut correct = 0usize;
    let mut calib = Vec::new();

    for (m, f, hw) in eval_games {
        let mx = base_probs(m, f, poisson, rapm, gbt, 300);
        let p = meta.predict(&mx).clamp(0.01, 0.99);
        let y = if *hw { 1.0 } else { 0.0 };
        brier += (p - y).powi(2);
        ll += -(y * p.ln() + (1.0 - y) * (1.0 - p).ln());
        if (p > 0.5) == *hw { correct += 1; }
        calib.push((p, y));
    }

    let n = eval_games.len() as f64;
    (brier / n, ll / n, correct as f64 / eval_games.len() as f64, calib)
}

/// Full training + walk-forward backtest.
pub async fn train_and_evaluate(pool: &SqlitePool) -> Result<(MlModelState, Vec<FoldResult>)> {
    tracing::info!("Starting ML training pipeline...");

    let rows = sqlx::query(
        r#"SELECT id, home_team_id, away_team_id, home_team_name, away_team_name,
                  sport, league, match_date, status, home_score, away_score,
                  created_at, updated_at
           FROM matches
           WHERE sport = 'basketball' AND status = 'finished'
             AND home_score IS NOT NULL AND away_score IS NOT NULL
           ORDER BY match_date ASC"#
    ).fetch_all(pool).await?;

    tracing::info!("Found {} finished NBA games", rows.len());

    if rows.len() < 50 {
        tracing::warn!("Not enough data to train (<50 games). Returning untrained model.");
        return Ok((MlModelState::new(), vec![]));
    }

    // Build features for all games
    let mut all_games: Vec<(Match, [f64; N_FEATURES], bool)> = Vec::new();
    for row in &rows {
        match parse_match(row) {
            Ok(m) => {
                match build_features(pool, &m).await {
                    Ok(feat) => {
                        let hw = m.home_score.unwrap_or(0) > m.away_score.unwrap_or(0);
                        all_games.push((m, feat.0, hw));
                    }
                    Err(e) => tracing::debug!("Feature build failed for {}: {}", row.get::<String, _>("id"), e),
                }
            }
            Err(e) => tracing::debug!("Parse failed: {}", e),
        }
    }

    tracing::info!("Features built for {}/{} games", all_games.len(), rows.len());

    let years: Vec<i32> = all_games.iter().map(|(m, _, _)| {
        m.match_date.format("%Y").to_string().parse::<i32>().unwrap_or(2020)
    }).collect();

    let min_yr = *years.iter().min().unwrap_or(&2020);
    let max_yr = *years.iter().max().unwrap_or(&2024);

    let mut folds: Vec<FoldResult> = Vec::new();

    for eval_yr in (min_yr + 2)..=max_yr {
        let train_idx: Vec<usize> = years.iter().enumerate().filter(|(_, &y)| y < eval_yr).map(|(i, _)| i).collect();
        let eval_idx: Vec<usize> = years.iter().enumerate().filter(|(_, &y)| y == eval_yr).map(|(i, _)| i).collect();

        if train_idx.len() < 30 || eval_idx.is_empty() { continue; }

        let poisson_data: Vec<(String, String, f64, f64)> = train_idx.iter().filter_map(|&i| {
            let (m, _, _) = &all_games[i];
            Some((m.home_team_id.clone(), m.away_team_id.clone(), m.home_score? as f64, m.away_score? as f64))
        }).collect();
        let rapm_data: Vec<(String, String, f64)> = train_idx.iter().filter_map(|&i| {
            let (m, _, _) = &all_games[i];
            Some((m.home_team_id.clone(), m.away_team_id.clone(), m.home_score? as f64 - m.away_score? as f64))
        }).collect();
        let gbt_data: Vec<([f64; N_FEATURES], f64)> = train_idx.iter()
            .map(|&i| { let (_, f, hw) = &all_games[i]; (*f, if *hw { 1.0 } else { 0.0 }) }).collect();

        let mut poisson = PoissonModel::new(); poisson.fit(&poisson_data);
        let mut rapm = RapmModel::new(); rapm.fit(&rapm_data);
        let mut gbt = GradientBoostedModel::new(); gbt.fit(&gbt_data);

        // Build meta-training data on train set
        let meta_train: Vec<([f64; META_FEATURES], f64)> = train_idx.iter().map(|&i| {
            let (m, f, hw) = &all_games[i];
            let mx = base_probs(m, f, &poisson, &rapm, &gbt, 200);
            (mx, if *hw { 1.0 } else { 0.0 })
        }).collect();

        let mut meta = MetaLearner::new();
        meta.fit(&meta_train);

        let eval_games: Vec<_> = eval_idx.iter().map(|&i| all_games[i].clone()).collect();
        let (brier, logloss, acc, _) = eval_fold(&eval_games, &poisson, &rapm, &gbt, &meta);

        tracing::info!("Fold {} (year {}): n={} brier={:.4} ll={:.4} acc={:.1}%",
            eval_yr - min_yr, eval_yr, eval_idx.len(), brier, logloss, acc * 100.0);

        folds.push(FoldResult {
            fold: (eval_yr - min_yr) as i32,
            year: eval_yr,
            n_games: eval_idx.len() as i32,
            brier_score: brier,
            log_loss: logloss,
            accuracy: acc,
        });
    }

    // Final train on all data
    tracing::info!("Final training on all {} games...", all_games.len());

    let all_poisson: Vec<(String, String, f64, f64)> = all_games.iter().filter_map(|(m, _, _)| {
        Some((m.home_team_id.clone(), m.away_team_id.clone(), m.home_score? as f64, m.away_score? as f64))
    }).collect();
    let all_rapm: Vec<(String, String, f64)> = all_games.iter().filter_map(|(m, _, _)| {
        Some((m.home_team_id.clone(), m.away_team_id.clone(), m.home_score? as f64 - m.away_score? as f64))
    }).collect();
    let all_gbt: Vec<([f64; N_FEATURES], f64)> = all_games.iter()
        .map(|(_, f, hw)| (*f, if *hw { 1.0 } else { 0.0 })).collect();

    let mut poisson = PoissonModel::new(); poisson.fit(&all_poisson);
    let mut rapm = RapmModel::new(); rapm.fit(&all_rapm);
    let mut gbt = GradientBoostedModel::new(); gbt.fit(&all_gbt);

    let all_meta: Vec<([f64; META_FEATURES], f64)> = all_games.iter().map(|(m, f, hw)| {
        let mx = base_probs(m, f, &poisson, &rapm, &gbt, 200);
        (mx, if *hw { 1.0 } else { 0.0 })
    }).collect();

    let mut meta = MetaLearner::new();
    meta.fit(&all_meta);

    // Calibrate on full training set (in-sample, better than nothing)
    let calib_data: Vec<(f64, f64)> = all_meta.iter().map(|(x, y)| (meta.predict(x), *y)).collect();
    let mut calibrator = IsotonicCalibrator::new();
    calibrator.fit(&calib_data);

    let version = format!("ml_v1.0_{}", chrono::Utc::now().format("%Y%m%d_%H%M"));
    let state = MlModelState { poisson, rapm, gbt, meta, calibrator, model_version: version };

    Ok((state, folds))
}
