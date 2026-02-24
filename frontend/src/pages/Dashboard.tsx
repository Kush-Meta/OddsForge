import React, { useState, useEffect } from 'react';
import { BarChart3, Calendar, TrendingUp, AlertCircle, RefreshCw } from 'lucide-react';
import { apiService, UpcomingMatchWithPrediction } from '../services/api';
import { format } from 'date-fns';

const LEAGUE_TABS = [
  { key: 'all',        label: 'All Sports' },
  { key: 'football',   label: '⚽ EPL' },
  { key: 'basketball', label: '🏀 NBA' },
];

const getLeagueIcon = (league: string) => {
  switch (league.toLowerCase()) {
    case 'epl': case 'premier league': return '⚽';
    case 'champions league': return '🏆';
    case 'nba': return '🏀';
    default: return '🏟️';
  }
};

const formatProb = (p: number) => `${(p * 100).toFixed(1)}%`;

// ── Match Card ─────────────────────────────────────────────────────────────
interface MatchCardProps { match: UpcomingMatchWithPrediction; }

const MatchCard: React.FC<MatchCardProps> = ({ match }) => {
  const m = match.match_info;
  const p = match.prediction;
  const matchDate = new Date(m.match_date);

  const homeWins = p && p.home_win_probability > p.away_win_probability &&
    (!p.draw_probability || p.home_win_probability > p.draw_probability);
  const awayWins = p && p.away_win_probability > p.home_win_probability &&
    (!p.draw_probability || p.away_win_probability > p.draw_probability);

  const homePct  = p ? Math.round(p.home_win_probability * 100) : 50;
  const awayPct  = p ? Math.round(p.away_win_probability * 100) : 50;
  const drawPct  = p?.draw_probability ? Math.round(p.draw_probability * 100) : 0;

  return (
    <div className="match-card">
      <div className="match-header">
        <span className="match-league">
          {getLeagueIcon(m.league)} {m.league}
        </span>
        <span className="match-date">{format(matchDate, 'MMM d, HH:mm')}</span>
      </div>

      <div className="match-teams">
        <div className={`team ${homeWins ? 'team-favorite' : ''}`}>
          <span className="team-name">{m.home_team_name}</span>
          {p && <span className="win-prob">{formatProb(p.home_win_probability)}</span>}
        </div>

        <div className="match-vs">
          {m.sport === 'football' && p?.draw_probability && (
            <div className="draw-prob">Draw {formatProb(p.draw_probability)}</div>
          )}
          <span>VS</span>
        </div>

        <div className={`team ${awayWins ? 'team-favorite' : ''}`} style={{ alignItems: 'flex-end', textAlign: 'right' }}>
          <span className="team-name">{m.away_team_name}</span>
          {p && <span className="win-prob">{formatProb(p.away_win_probability)}</span>}
        </div>
      </div>

      {/* Animated probability bars */}
      {p && (
        <div className="prob-bars">
          <div className="prob-bar-home" style={{ width: `${homePct}%` }} />
          {drawPct > 0 && <div className="prob-bar-draw" style={{ width: `${drawPct}%` }} />}
          <div className="prob-bar-away" style={{ width: `${awayPct}%` }} />
        </div>
      )}

      {p && (
        <div className="match-footer">
          <div className="confidence-bar">
            <span className="confidence-label">
              Model confidence: {formatProb(p.confidence_score)}
            </span>
            <div className="confidence-meter">
              <div className="confidence-fill" style={{ width: `${p.confidence_score * 100}%` }} />
            </div>
          </div>

          <div className="prediction-summary">
            <TrendingUp size={13} />
            <span>
              Favours:{' '}
              <strong>
                {homeWins ? m.home_team_name : awayWins ? m.away_team_name : 'Draw'}
              </strong>
            </span>
          </div>
        </div>
      )}
    </div>
  );
};

// ── Dashboard Page ─────────────────────────────────────────────────────────
const Dashboard: React.FC = () => {
  const [matches, setMatches]     = useState<UpcomingMatchWithPrediction[]>([]);
  const [loading, setLoading]     = useState(true);
  const [error, setError]         = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState('all');

  const fetchMatches = async (sport?: string) => {
    setLoading(true);
    setError(null);
    try {
      const data = await apiService.getUpcomingMatches(sport === 'all' ? undefined : sport, 30);
      setMatches(data);
    } catch {
      setError('Failed to fetch matches. Make sure the backend is running on port 3000.');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { fetchMatches(activeTab); }, [activeTab]);

  const withPredictions = matches.filter(m => m.prediction);
  const highConfidence  = matches.filter(m => m.prediction && m.prediction.confidence_score > 0.7);

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <BarChart3 size={32} />
          <div>
            <h1>Dashboard</h1>
            <p>Upcoming matches with ELO-powered predictions</p>
          </div>
        </div>
        <div className="page-actions">
          <button onClick={() => fetchMatches(activeTab)} className="refresh-btn">
            <RefreshCw size={16} />
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <div className="error-banner">
          <AlertCircle size={18} />
          <span>{error}</span>
        </div>
      )}

      {/* Stats */}
      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-icon">📊</div>
          <div>
            <div className="stat-value">{matches.length}</div>
            <div className="stat-label">Upcoming Matches</div>
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-icon">🎯</div>
          <div>
            <div className="stat-value">{withPredictions.length}</div>
            <div className="stat-label">With Predictions</div>
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-icon">⚡</div>
          <div>
            <div className="stat-value">{highConfidence.length}</div>
            <div className="stat-label">High Confidence</div>
          </div>
        </div>
      </div>

      {/* League filter tabs */}
      <div className="league-tabs">
        {LEAGUE_TABS.map(tab => (
          <button
            key={tab.key}
            className={`league-tab ${activeTab === tab.key ? 'league-tab-active' : ''}`}
            onClick={() => setActiveTab(tab.key)}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Matches */}
      <div className="matches-section">
        <h2>Upcoming Matches</h2>

        {loading ? (
          <div className="loading">
            <RefreshCw className="spin" size={32} />
            <p>Loading matches...</p>
          </div>
        ) : matches.length === 0 ? (
          <div className="empty-state">
            <Calendar size={48} />
            <h3>No upcoming matches</h3>
            <p>Try refreshing or selecting a different league</p>
          </div>
        ) : (
          <div className="matches-grid">
            {matches.map(match => (
              <MatchCard key={match.match_info.id} match={match} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default Dashboard;
