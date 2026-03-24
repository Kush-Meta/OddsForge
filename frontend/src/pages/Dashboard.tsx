import React, { useState, useEffect } from 'react';
import { BarChart3, Calendar, AlertCircle, RefreshCw } from 'lucide-react';
import { apiService, UpcomingMatchWithPrediction } from '../services/api';
import GameCard from '../components/GameCard';

const LEAGUE_TABS = [
  { key: 'all',        label: 'All Sports' },
  { key: 'football',   label: '⚽ EPL' },
  { key: 'basketball', label: '🏀 NBA' },
];

const Dashboard: React.FC = () => {
  const [matches, setMatches]     = useState<UpcomingMatchWithPrediction[]>([]);
  const [loading, setLoading]     = useState(true);
  const [error, setError]         = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState('all');

  const fetchMatches = async (sport?: string, silent = false) => {
    if (!silent) setLoading(true);
    setError(null);
    try {
      const data = await apiService.getUpcomingMatches(sport === 'all' ? undefined : sport, 30);
      setMatches(data);
    } catch {
      if (!silent) setError('Failed to fetch matches. Make sure the backend is running on port 3000.');
    } finally {
      if (!silent) setLoading(false);
    }
  };

  useEffect(() => { fetchMatches(activeTab); }, [activeTab]);

  useEffect(() => {
    const interval = setInterval(() => fetchMatches(activeTab, true), 60_000);
    return () => clearInterval(interval);
  }, [activeTab]);

  const withPredictions = matches.filter(m => m.prediction);
  const highConfidence  = matches.filter(m => m.prediction && m.prediction.confidence_score > 0.7);

  return (
    <div className="page">
      {/* Header */}
      <div className="page-header">
        <div className="page-title">
          <div className="page-title-icon">
            <BarChart3 size={26} />
          </div>
          <div>
            <h1>Dashboard</h1>
            <p>AI-powered match predictions with full transparency</p>
          </div>
        </div>
        <div className="page-actions">
          <button onClick={() => fetchMatches(activeTab)} className="refresh-btn">
            <RefreshCw size={15} />
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
        <h2>Upcoming Matches — click any card to reveal the algorithm</h2>

        {loading ? (
          <div className="loading">
            <div className="loading-ring" />
            <p>Loading matches…</p>
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
              <GameCard key={match.match_info.id} match={match} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default Dashboard;
