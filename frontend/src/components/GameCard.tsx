import React, { useState, useCallback } from 'react';
import { ChevronDown, Users } from 'lucide-react';
import { format } from 'date-fns';
import { UpcomingMatchWithPrediction, MatchAnalysis, apiService } from '../services/api';
import AlgorithmBreakdown from './AlgorithmBreakdown';
import LineupPanel from './LineupPanel';

interface Props {
  match: UpcomingMatchWithPrediction;
}

const getLeagueIcon = (league: string) => {
  switch (league.toLowerCase()) {
    case 'epl': case 'premier league': return '⚽';
    case 'champions league': return '🏆';
    case 'nba': return '🏀';
    default: return '🏟️';
  }
};

const confidenceClass = (score: number) => {
  if (score >= 0.7) return 'conf-high';
  if (score >= 0.5) return 'conf-medium';
  return 'conf-low';
};

const confidenceLabel = (score: number) => {
  if (score >= 0.7) return 'High confidence';
  if (score >= 0.5) return 'Medium confidence';
  return 'Low confidence';
};

const GameCard: React.FC<Props> = ({ match }) => {
  const m = match.match_info;
  const p = match.prediction;

  const [expanded, setExpanded] = useState(false);
  const [analysis, setAnalysis] = useState<MatchAnalysis | null>(null);
  const [loadingAnalysis, setLoadingAnalysis] = useState(false);
  const [analysisError, setAnalysisError] = useState(false);
  const [showLineup, setShowLineup] = useState(false);
  const isNba = m.sport === 'basketball';

  const homePct = p ? Math.round(p.home_win_probability * 100) : 50;
  const awayPct = p ? Math.round(p.away_win_probability * 100) : 50;
  const drawPct = p?.draw_probability ? Math.round(p.draw_probability * 100) : 0;

  const homeWins = p
    ? p.home_win_probability > p.away_win_probability &&
      (!p.draw_probability || p.home_win_probability > p.draw_probability)
    : false;
  const awayWins = p
    ? p.away_win_probability > p.home_win_probability &&
      (!p.draw_probability || p.away_win_probability > p.draw_probability)
    : false;

  const toggleExpand = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation();
    const next = !expanded;
    setExpanded(next);

    if (next && !analysis && !loadingAnalysis) {
      setLoadingAnalysis(true);
      setAnalysisError(false);
      try {
        const data = await apiService.getMatchAnalysis(m.id);
        setAnalysis(data);
      } catch {
        setAnalysisError(true);
      } finally {
        setLoadingAnalysis(false);
      }
    }
  }, [expanded, analysis, loadingAnalysis, m.id]);

  return (
    <div className={`game-card ${expanded ? 'expanded' : ''}`}>
      <div className="game-card-body" onClick={toggleExpand} style={{ cursor: 'pointer' }}>
        {/* Header */}
        <div className="game-card-header">
          <div className="game-league-badge">
            {getLeagueIcon(m.league)} {m.league}
          </div>
          <span className="game-date">
            {format(new Date(m.match_date), 'MMM d · HH:mm')}
          </span>
        </div>

        {/* Matchup */}
        <div className="game-matchup">
          <div className={`game-team game-team--home ${homeWins ? 'game-team--favorite' : ''}`}>
            <span className="game-team-name">{m.home_team_name}</span>
            {p && (
              <span className="game-team-prob">{homePct}%</span>
            )}
          </div>

          <div className="game-vs">
            <span className="game-vs-label">VS</span>
            {drawPct > 0 && (
              <span className="game-draw-badge">Draw {drawPct}%</span>
            )}
          </div>

          <div className={`game-team game-team--away ${awayWins ? 'game-team--favorite' : ''}`}>
            <span className="game-team-name">{m.away_team_name}</span>
            {p && (
              <span className="game-team-prob">{awayPct}%</span>
            )}
          </div>
        </div>

        {/* Probability spectrum bar */}
        {p && (
          <div className="prob-spectrum">
            <div
              className={`prob-spectrum-home ${homeWins ? 'home-winning' : 'neutral-bar'}`}
              style={{ width: `${homePct}%` }}
            />
            {drawPct > 0 && (
              <div className="draw-bar" style={{ width: `${drawPct}%` }} />
            )}
            <div
              className={`prob-spectrum-away ${awayWins ? 'away-winning' : 'neutral-bar'}`}
            />
          </div>
        )}

        {/* Footer */}
        <div className="game-card-footer">
          {p ? (
            <div className="confidence-pill">
              <div className={`confidence-dot ${confidenceClass(p.confidence_score)}`} />
              {confidenceLabel(p.confidence_score)} · {Math.round(p.confidence_score * 100)}%
            </div>
          ) : (
            <div className="confidence-pill">No prediction yet</div>
          )}

          <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
            {isNba && (
              <div
                className={`analysis-trigger ${showLineup ? 'open' : ''}`}
                onClick={e => { e.stopPropagation(); setShowLineup(s => !s); }}
                style={{ background: showLineup ? 'rgba(0,212,255,0.2)' : 'var(--cyan-dim)', borderColor: 'var(--border-cyan)', color: 'var(--cyan)' }}
              >
                <Users size={11} />
                <span>Lineup</span>
                <ChevronDown size={11} />
              </div>
            )}
            {p && (
              <div className={`analysis-trigger ${expanded ? 'open' : ''}`} onClick={toggleExpand}>
                <span>How?</span>
                <ChevronDown size={13} />
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Lineup panel — NBA only */}
      {isNba && showLineup && (
        <LineupPanel
          homeTeamId={m.home_team_id}
          awayTeamId={m.away_team_id}
          homeTeamName={m.home_team_name}
          awayTeamName={m.away_team_name}
        />
      )}

      {/* Algorithm breakdown panel */}
      {p && (
        <div className={`breakdown-panel ${expanded ? 'open' : ''}`}>
          {loadingAnalysis && (
            <div className="breakdown-loading">
              <div className="loading-ring" style={{ width: 24, height: 24, borderWidth: 2 }} />
              Loading analysis…
            </div>
          )}
          {analysisError && (
            <div className="breakdown-error">
              Analysis unavailable for this match.
            </div>
          )}
          {analysis && (
            <AlgorithmBreakdown
              analysis={analysis}
              homeName={m.home_team_name}
              awayName={m.away_team_name}
            />
          )}
        </div>
      )}
    </div>
  );
};

export default GameCard;
