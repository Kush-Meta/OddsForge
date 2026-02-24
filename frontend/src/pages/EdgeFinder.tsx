import React, { useState, useEffect } from 'react';
import { Target, TrendingUp, AlertTriangle, RefreshCw, Info } from 'lucide-react';
import { apiService, Edge } from '../services/api';
import { format } from 'date-fns';

const EdgeFinder: React.FC = () => {
  const [edges, setEdges] = useState<Edge[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchEdges = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await apiService.getPredictionEdges();
      setEdges(data);
    } catch (err) {
      setError('Failed to fetch market edges. Make sure predictions are generated.');
      console.error('Error fetching edges:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchEdges();
  }, []);

  const formatProbability = (prob: number) => `${(prob * 100).toFixed(1)}%`;
  const formatOdds = (odds: number) => odds.toFixed(2);
  const formatEdge = (edge: number) => `${(edge * 100).toFixed(1)}%`;

  const getEdgeSeverity = (edge: number) => {
    if (edge > 0.15) return 'edge-high';
    if (edge > 0.08) return 'edge-medium';
    return 'edge-low';
  };

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

  const impliedProbability = (odds: number) => 1 / odds;

  if (loading) {
    return (
      <div className="page">
        <div className="loading">
          <RefreshCw className="spin" size={32} />
          <p>Finding market edges...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <Target size={32} />
          <div>
            <h1>Edge Finder</h1>
            <p>Discover betting opportunities where our model disagrees with the market</p>
          </div>
        </div>
        
        <div className="page-actions">
          <button onClick={fetchEdges} className="refresh-btn">
            <RefreshCw size={18} />
            Refresh Edges
          </button>
        </div>
      </div>

      {error && (
        <div className="error-banner">
          <AlertTriangle size={20} />
          <span>{error}</span>
        </div>
      )}

      <div className="info-banner">
        <Info size={20} />
        <div>
          <strong>Note:</strong> Market odds shown are simulated for demonstration. 
          In production, these would be fetched from real betting APIs.
        </div>
      </div>

      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-icon">🎯</div>
          <div>
            <div className="stat-value">{edges.length}</div>
            <div className="stat-label">Total Edges Found</div>
          </div>
        </div>
        
        <div className="stat-card">
          <div className="stat-icon">🔥</div>
          <div>
            <div className="stat-value">
              {edges.filter(e => e.edge_value > 0.1).length}
            </div>
            <div className="stat-label">High Value Edges</div>
          </div>
        </div>
        
        <div className="stat-card">
          <div className="stat-icon">📊</div>
          <div>
            <div className="stat-value">
              {edges.length > 0 ? formatEdge(Math.max(...edges.map(e => e.edge_value))) : '0%'}
            </div>
            <div className="stat-label">Max Edge Value</div>
          </div>
        </div>
      </div>

      <div className="edges-section">
        <h2>Market Edges</h2>
        
        {edges.length === 0 ? (
          <div className="empty-state">
            <Target size={48} />
            <h3>No significant edges found</h3>
            <p>Try generating predictions first or check back later for new opportunities</p>
          </div>
        ) : (
          <div className="edges-grid">
            {edges.map((edge) => {
              const matchDate = new Date(edge.match_info.match_date);
              const homeImplied = impliedProbability(edge.market_home_odds);
              const awayImplied = impliedProbability(edge.market_away_odds);
              const drawImplied = edge.market_draw_odds ? impliedProbability(edge.market_draw_odds) : 0;
              
              return (
                <div key={edge.match_id} className={`edge-card ${getEdgeSeverity(edge.edge_value)}`}>
                  <div className="edge-header">
                    <div className="match-info">
                      <span className="match-league">
                        {getLeagueIcon(edge.match_info.league)} {edge.match_info.league}
                      </span>
                      <span className="match-date">
                        {format(matchDate, 'MMM d, HH:mm')}
                      </span>
                    </div>
                    <div className={`edge-badge ${getEdgeSeverity(edge.edge_value)}`}>
                      {formatEdge(edge.edge_value)} Edge
                    </div>
                  </div>

                  <div className="match-teams">
                    <div className="team-name">{edge.match_info.home_team_name}</div>
                    <span className="vs">VS</span>
                    <div className="team-name">{edge.match_info.away_team_name}</div>
                  </div>

                  <div className="predictions-comparison">
                    <div className="comparison-section">
                      <h4>Our Model</h4>
                      <div className="predictions">
                        <div className="prediction">
                          <span>Home:</span>
                          <span className="value">{formatProbability(edge.our_prediction.home_win_probability)}</span>
                        </div>
                        <div className="prediction">
                          <span>Away:</span>
                          <span className="value">{formatProbability(edge.our_prediction.away_win_probability)}</span>
                        </div>
                        {edge.our_prediction.draw_probability && (
                          <div className="prediction">
                            <span>Draw:</span>
                            <span className="value">{formatProbability(edge.our_prediction.draw_probability)}</span>
                          </div>
                        )}
                      </div>
                    </div>

                    <div className="comparison-section">
                      <h4>Market (Implied)</h4>
                      <div className="predictions">
                        <div className="prediction">
                          <span>Home:</span>
                          <span className="value">{formatProbability(homeImplied)}</span>
                        </div>
                        <div className="prediction">
                          <span>Away:</span>
                          <span className="value">{formatProbability(awayImplied)}</span>
                        </div>
                        {edge.market_draw_odds && (
                          <div className="prediction">
                            <span>Draw:</span>
                            <span className="value">{formatProbability(drawImplied)}</span>
                          </div>
                        )}
                      </div>
                    </div>
                  </div>

                  <div className="market-odds">
                    <h4>Market Odds</h4>
                    <div className="odds-row">
                      <div className="odd">
                        <span>Home:</span>
                        <span className="odd-value">{formatOdds(edge.market_home_odds)}</span>
                      </div>
                      <div className="odd">
                        <span>Away:</span>
                        <span className="odd-value">{formatOdds(edge.market_away_odds)}</span>
                      </div>
                      {edge.market_draw_odds && (
                        <div className="odd">
                          <span>Draw:</span>
                          <span className="odd-value">{formatOdds(edge.market_draw_odds)}</span>
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="edge-footer">
                    <div className="confidence-info">
                      <TrendingUp size={14} />
                      <span>
                        Confidence: {formatProbability(edge.our_prediction.confidence_score)}
                      </span>
                    </div>
                    <div className="model-info">
                      Model: {edge.our_prediction.model_version}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {edges.length > 0 && (
        <div className="disclaimer">
          <AlertTriangle size={16} />
          <p>
            <strong>Risk Warning:</strong> Sports betting involves risk. 
            These are model predictions, not guarantees. 
            Always bet responsibly and within your means.
          </p>
        </div>
      )}
    </div>
  );
};

export default EdgeFinder;