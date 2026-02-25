import React, { useState, useEffect, useMemo } from 'react';
import { Target, TrendingUp, AlertTriangle, RefreshCw, Info, ChevronUp, ChevronDown, Zap } from 'lucide-react';
import { apiService, Edge } from '../services/api';
import { format, formatDistanceToNow } from 'date-fns';

type SortKey = 'match' | 'our_home' | 'market_home' | 'edge' | 'confidence';
type SortDir = 'asc' | 'desc';

const getEdgeBadgeClass = (edge: number) => {
  if (edge > 0.15) return 'edge-badge edge-badge-high';
  if (edge > 0.08) return 'edge-badge edge-badge-medium';
  return 'edge-badge edge-badge-low';
};

const fmt = (n: number, decimals = 1) => `${(n * 100).toFixed(decimals)}%`;
const fmtOdds = (n: number) => n.toFixed(2);

const EdgeFinder: React.FC = () => {
  const [edges, setEdges]       = useState<Edge[]>([]);
  const [loading, setLoading]   = useState(true);
  const [error, setError]       = useState<string | null>(null);
  const [sortKey, setSortKey]   = useState<SortKey>('edge');
  const [sortDir, setSortDir]   = useState<SortDir>('desc');

  const fetchEdges = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await apiService.getPredictionEdges();
      setEdges(data);
    } catch {
      setError('Failed to fetch edges. Ensure the backend is running and predictions are generated.');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { fetchEdges(); }, []);

  const handleSort = (key: SortKey) => {
    if (sortKey === key) setSortDir(d => d === 'asc' ? 'desc' : 'asc');
    else { setSortKey(key); setSortDir('desc'); }
  };

  const sorted = useMemo(() => {
    const arr = [...edges];
    arr.sort((a, b) => {
      let va: number, vb: number;
      switch (sortKey) {
        case 'match':       va = a.match_info.home_team_name.localeCompare(b.match_info.home_team_name); return sortDir === 'asc' ? va : -va;
        case 'our_home':    va = a.our_prediction.home_win_probability; vb = b.our_prediction.home_win_probability; break;
        case 'market_home': va = 1/a.market_home_odds; vb = 1/b.market_home_odds; break;
        case 'edge':        va = a.edge_value; vb = b.edge_value; break;
        case 'confidence':  va = a.our_prediction.confidence_score; vb = b.our_prediction.confidence_score; break;
        default:            va = a.edge_value; vb = b.edge_value;
      }
      return sortDir === 'asc' ? va - vb : vb - va;
    });
    return arr;
  }, [edges, sortKey, sortDir]);

  const SortIcon: React.FC<{ col: SortKey }> = ({ col }) => {
    if (sortKey !== col) return <ChevronDown size={12} className="sort-indicator" />;
    return sortDir === 'asc'
      ? <ChevronUp size={12} className="sort-indicator" />
      : <ChevronDown size={12} className="sort-indicator" />;
  };

  const maxEdge = edges.length > 0 ? Math.max(...edges.map(e => e.edge_value)) : 0;

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <Target size={32} />
          <div>
            <h1>Edge Finder</h1>
            <p>Matches where our model disagrees with market odds by &gt;5%</p>
          </div>
        </div>
        <div className="page-actions">
          <button onClick={fetchEdges} className="refresh-btn">
            <RefreshCw size={16} />
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <div className="error-banner">
          <AlertTriangle size={18} />
          <span>{error}</span>
        </div>
      )}

      {(() => {
        const liveCount = edges.filter(e => e.is_live_odds).length;
        const simCount  = edges.length - liveCount;
        return (
          <div className="info-banner">
            <Info size={18} />
            <div>
              {liveCount > 0
                ? <><strong style={{ color: 'var(--accent)' }}><Zap size={13} style={{ display: 'inline', verticalAlign: 'middle' }} /> Live market odds</strong> from The Odds API ({liveCount} match{liveCount !== 1 ? 'es' : ''}).{simCount > 0 ? ` ${simCount} simulated.` : ''} Edge = Our probability − devigged market probability.</>
                : <><strong>Simulated market odds</strong> — live odds will appear once The Odds API syncs. Edge = (Our probability) − (Market implied probability).</>
              }
            </div>
          </div>
        );
      })()}

      {/* Summary stats */}
      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-icon">🎯</div>
          <div>
            <div className="stat-value">{edges.length}</div>
            <div className="stat-label">Edges Found</div>
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-icon">🔥</div>
          <div>
            <div className="stat-value">{edges.filter(e => e.edge_value > 0.10).length}</div>
            <div className="stat-label">High Value (&gt;10%)</div>
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-icon">📊</div>
          <div>
            <div className="stat-value">{fmt(maxEdge, 1)}</div>
            <div className="stat-label">Best Edge</div>
          </div>
        </div>
      </div>

      <div className="edges-section">
        <h2>Market Edges</h2>

        {loading ? (
          <div className="loading">
            <RefreshCw className="spin" size={32} />
            <p>Scanning for edges...</p>
          </div>
        ) : edges.length === 0 ? (
          <div className="empty-state">
            <Target size={48} />
            <h3>No edges found yet</h3>
            <p>Live market odds sync from The Odds API every 12 hours. Once synced, this page shows matches where our model disagrees with the market by &gt;3%.</p>
          </div>
        ) : (
          <div className="edges-table-wrapper">
            <table className="edges-table">
              <thead>
                <tr>
                  <th onClick={() => handleSort('match')}>Match <SortIcon col="match" /></th>
                  <th>League</th>
                  <th onClick={() => handleSort('our_home')}>Our Home % <SortIcon col="our_home" /></th>
                  <th onClick={() => handleSort('market_home')}>Market Home % <SortIcon col="market_home" /></th>
                  <th>Market Odds</th>
                  <th onClick={() => handleSort('edge')}>Edge % <SortIcon col="edge" /></th>
                  <th onClick={() => handleSort('confidence')}>Confidence <SortIcon col="confidence" /></th>
                </tr>
              </thead>
              <tbody>
                {sorted.map(edge => {
                  const marketHomeProb = 1 / edge.market_home_odds;
                  const marketAwayProb = 1 / edge.market_away_odds;
                  const homeEdge = edge.our_prediction.home_win_probability - marketHomeProb;
                  const awayEdge = edge.our_prediction.away_win_probability - marketAwayProb;
                  const bestEdge = Math.abs(homeEdge) > Math.abs(awayEdge) ? homeEdge : awayEdge;
                  const matchDate = new Date(edge.match_info.match_date);

                  return (
                    <tr key={edge.match_id}>
                      <td>
                        <div className="match-cell">
                          <span className="match-name">
                            {edge.match_info.home_team_name} vs {edge.match_info.away_team_name}
                          </span>
                          <span className="match-meta">{format(matchDate, 'MMM d, HH:mm')}</span>
                        </div>
                      </td>
                      <td style={{ color: 'var(--accent-light)', fontSize: '0.82rem' }}>
                        {edge.match_info.league}
                      </td>
                      <td>
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
                          <span>H: <strong>{fmt(edge.our_prediction.home_win_probability)}</strong></span>
                          <span style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>
                            A: {fmt(edge.our_prediction.away_win_probability)}
                            {edge.our_prediction.draw_probability
                              ? ` · D: ${fmt(edge.our_prediction.draw_probability)}`
                              : ''}
                          </span>
                        </div>
                      </td>
                      <td>
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
                          <span>H: <strong>{fmt(marketHomeProb)}</strong></span>
                          <span style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>
                            A: {fmt(marketAwayProb)}
                          </span>
                        </div>
                      </td>
                      <td>
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 2, fontSize: '0.82rem' }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
                            <span>H: {fmtOdds(edge.market_home_odds)}</span>
                            {edge.is_live_odds && (
                              <span style={{
                                background: 'var(--accent)',
                                color: '#000',
                                fontSize: '0.65rem',
                                fontWeight: 700,
                                padding: '1px 5px',
                                borderRadius: 4,
                                letterSpacing: '0.03em',
                              }}>LIVE</span>
                            )}
                          </div>
                          <span style={{ color: 'var(--text-muted)' }}>A: {fmtOdds(edge.market_away_odds)}</span>
                          {edge.market_draw_odds && (
                            <span style={{ color: 'var(--text-muted)' }}>D: {fmtOdds(edge.market_draw_odds)}</span>
                          )}
                          {edge.bookmaker && (
                            <span style={{ color: 'var(--accent-light)', fontSize: '0.72rem' }}>
                              {edge.bookmaker}
                              {edge.odds_fetched_at && (
                                <> · {formatDistanceToNow(new Date(edge.odds_fetched_at), { addSuffix: true })}</>
                              )}
                            </span>
                          )}
                        </div>
                      </td>
                      <td>
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                          <span className={getEdgeBadgeClass(edge.edge_value)}>
                            {fmt(edge.edge_value)}
                          </span>
                          <span
                            className={bestEdge > 0 ? 'edge-positive' : 'edge-negative'}
                            style={{ fontSize: '0.75rem' }}
                          >
                            {bestEdge > 0 ? '▲' : '▼'} {fmt(Math.abs(bestEdge), 1)}
                          </span>
                        </div>
                      </td>
                      <td>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                          <TrendingUp size={13} style={{ color: 'var(--accent)', flexShrink: 0, marginTop: 2 }} />
                          <span>{fmt(edge.our_prediction.confidence_score)}</span>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {edges.length > 0 && (
        <div className="disclaimer">
          <AlertTriangle size={16} style={{ flexShrink: 0, marginTop: 2 }} />
          <p>
            <strong>Risk Warning:</strong> Sports betting involves financial risk.
            These predictions are model outputs and not guarantees.
            Always bet responsibly and within your means.
          </p>
        </div>
      )}
    </div>
  );
};

export default EdgeFinder;
