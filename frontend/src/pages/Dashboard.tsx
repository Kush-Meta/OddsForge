import React, { useState, useEffect } from 'react';
import { Calendar, Trophy, TrendingUp, AlertCircle, RefreshCw } from 'lucide-react';
import { apiService, UpcomingMatchWithPrediction } from '../services/api';
import { format } from 'date-fns';

const Dashboard: React.FC = () => {
  const [matches, setMatches] = useState<UpcomingMatchWithPrediction[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedSport, setSelectedSport] = useState<string>('all');

  const fetchMatches = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await apiService.getUpcomingMatches(
        selectedSport === 'all' ? undefined : selectedSport,
        20
      );
      setMatches(data);
    } catch (err) {
      setError('Failed to fetch matches. Make sure the backend is running.');
      console.error('Error fetching matches:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchMatches();
  }, [selectedSport]);

  const getWinnerPrediction = (prediction?: { home_win_probability: number; away_win_probability: number; draw_probability?: number }) => {
    if (!prediction) return null;
    
    const { home_win_probability, away_win_probability, draw_probability } = prediction;
    
    if (draw_probability && draw_probability > home_win_probability && draw_probability > away_win_probability) {
      return { type: 'draw', probability: draw_probability };
    } else if (home_win_probability > away_win_probability) {
      return { type: 'home', probability: home_win_probability };
    } else {
      return { type: 'away', probability: away_win_probability };
    }
  };

  const formatProbability = (prob: number) => `${(prob * 100).toFixed(1)}%`;

  const getLeagueIcon = (league: string) => {
    switch (league.toLowerCase()) {
      case 'epl':
      case 'premier league':
        return '⚽';
      case 'champions league':
        return '🏆';
      case 'nba':
        return '🏀';
      default:
        return '🏟️';
    }
  };

  if (loading) {
    return (
      <div className="page">
        <div className="loading">
          <RefreshCw className="spin" size={32} />
          <p>Loading matches...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <BarChart3 size={32} />
          <div>
            <h1>Dashboard</h1>
            <p>Upcoming matches with AI predictions</p>
          </div>
        </div>
        
        <div className="page-actions">
          <select 
            value={selectedSport} 
            onChange={(e) => setSelectedSport(e.target.value)}
            className="sport-filter"
          >
            <option value="all">All Sports</option>
            <option value="football">Football</option>
            <option value="basketball">Basketball</option>
          </select>
          <button onClick={fetchMatches} className="refresh-btn">
            <RefreshCw size={18} />
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <div className="error-banner">
          <AlertCircle size={20} />
          <span>{error}</span>
        </div>
      )}

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
            <div className="stat-value">
              {matches.filter(m => m.prediction).length}
            </div>
            <div className="stat-label">With Predictions</div>
          </div>
        </div>
        
        <div className="stat-card">
          <div className="stat-icon">⚡</div>
          <div>
            <div className="stat-value">
              {matches.filter(m => m.prediction && m.prediction.confidence_score > 0.7).length}
            </div>
            <div className="stat-label">High Confidence</div>
          </div>
        </div>
      </div>

      <div className="matches-section">
        <h2>Upcoming Matches</h2>
        
        {matches.length === 0 ? (
          <div className="empty-state">
            <Calendar size={48} />
            <h3>No upcoming matches</h3>
            <p>Try refreshing or check back later</p>
          </div>
        ) : (
          <div className="matches-grid">
            {matches.map((match) => {
              const prediction = getWinnerPrediction(match.prediction);
              const matchDate = new Date(match.match_info.match_date);
              
              return (
                <div key={match.match_info.id} className="match-card">
                  <div className="match-header">
                    <span className="match-league">
                      {getLeagueIcon(match.match_info.league)} {match.match_info.league}
                    </span>
                    <span className="match-date">
                      {format(matchDate, 'MMM d, HH:mm')}
                    </span>
                  </div>

                  <div className="match-teams">
                    <div className={`team ${prediction?.type === 'home' ? 'team-favorite' : ''}`}>
                      <span className="team-name">{match.match_info.home_team_name}</span>
                      {match.prediction && (
                        <span className="win-prob">
                          {formatProbability(match.prediction.home_win_probability)}
                        </span>
                      )}
                    </div>
                    
                    <div className="match-vs">
                      {match.match_info.sport === 'football' && match.prediction?.draw_probability && (
                        <div className="draw-prob">
                          Draw: {formatProbability(match.prediction.draw_probability)}
                        </div>
                      )}
                      <span>VS</span>
                    </div>
                    
                    <div className={`team ${prediction?.type === 'away' ? 'team-favorite' : ''}`}>
                      <span className="team-name">{match.match_info.away_team_name}</span>
                      {match.prediction && (
                        <span className="win-prob">
                          {formatProbability(match.prediction.away_win_probability)}
                        </span>
                      )}
                    </div>
                  </div>

                  {match.prediction && (
                    <div className="match-footer">
                      <div className="confidence-bar">
                        <span className="confidence-label">
                          Confidence: {formatProbability(match.prediction.confidence_score)}
                        </span>
                        <div className="confidence-meter">
                          <div 
                            className="confidence-fill"
                            style={{ width: `${match.prediction.confidence_score * 100}%` }}
                          />
                        </div>
                      </div>
                      
                      {prediction && (
                        <div className="prediction-summary">
                          <TrendingUp size={14} />
                          <span>
                            Favors: {prediction.type === 'home' ? match.match_info.home_team_name : 
                                   prediction.type === 'away' ? match.match_info.away_team_name : 'Draw'} 
                            ({formatProbability(prediction.probability)})
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};

// Missing import
import { BarChart3 } from 'lucide-react';

export default Dashboard;