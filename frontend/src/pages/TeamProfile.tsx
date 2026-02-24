import React, { useState, useEffect, useMemo } from 'react';
import { Users, RefreshCw, AlertCircle } from 'lucide-react';
import {
  LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer
} from 'recharts';
import { apiService, Team } from '../services/api';
import { format } from 'date-fns';

// ── Types ──────────────────────────────────────────────────────────────────
interface Match {
  id: string;
  home_team_id: string;
  away_team_id: string;
  home_team_name: string;
  away_team_name: string;
  sport: string;
  league: string;
  match_date: string;
  status: string;
  home_score?: number;
  away_score?: number;
}

interface TeamStats {
  id: string;
  team_id: string;
  season: string;
  matches_played: number;
  wins: number;
  draws?: number;
  losses: number;
  goals_for?: number;
  goals_against?: number;
  points_for?: number;
  points_against?: number;
  form: string;
}

interface EloPoint {
  team_id: string;
  date: string;
  elo_rating: number;
  match_id?: string;
}

interface TeamProfileData {
  team: Team;
  current_stats: TeamStats;
  recent_matches: Match[];
  elo_history: EloPoint[];
}

// ── Helpers ────────────────────────────────────────────────────────────────
const getEmoji = (sport: string, league: string) => {
  if (league === 'NBA') return '🏀';
  if (sport === 'football') return '⚽';
  return '🏟️';
};

const getMatchResult = (match: Match, teamId: string): 'W' | 'D' | 'L' | null => {
  if (match.home_score == null || match.away_score == null) return null;
  const isHome = match.home_team_id === teamId;
  const teamScore = isHome ? match.home_score : match.away_score;
  const oppScore  = isHome ? match.away_score : match.home_score;
  if (teamScore > oppScore) return 'W';
  if (teamScore < oppScore) return 'L';
  return 'D';
};

// ── Custom Tooltip ─────────────────────────────────────────────────────────
const EloTooltip: React.FC<any> = ({ active, payload, label }) => {
  if (!active || !payload?.length) return null;
  return (
    <div style={{
      background: 'var(--bg-tertiary)',
      border: '1px solid var(--border)',
      borderRadius: 8,
      padding: '10px 14px',
      fontSize: '0.82rem',
    }}>
      <div style={{ color: 'var(--text-muted)', marginBottom: 4 }}>{label}</div>
      <div style={{ color: 'var(--accent-light)', fontWeight: 700 }}>
        ELO: {payload[0].value?.toFixed(0)}
      </div>
    </div>
  );
};

// ── TeamProfile Page ───────────────────────────────────────────────────────
const TeamProfile: React.FC = () => {
  const [teams, setTeams]             = useState<Team[]>([]);
  const [selected, setSelected]       = useState<string | null>(null);
  const [profile, setProfile]         = useState<TeamProfileData | null>(null);
  const [loadingTeams, setLoadingTeams] = useState(true);
  const [loadingProfile, setLoadingProfile] = useState(false);
  const [error, setError]             = useState<string | null>(null);
  const [search, setSearch]           = useState('');

  // Load all teams
  useEffect(() => {
    (async () => {
      try {
        const data = await apiService.getAllTeams();
        setTeams(data);
        // Auto-select Arsenal (first EPL team)
        const arsenal = data.find(t => t.name === 'Arsenal');
        if (arsenal) setSelected(arsenal.id);
      } catch {
        setError('Failed to load teams. Make sure the backend is running.');
      } finally {
        setLoadingTeams(false);
      }
    })();
  }, []);

  // Load profile when selection changes
  useEffect(() => {
    if (!selected) return;
    setLoadingProfile(true);
    setProfile(null);
    (async () => {
      try {
        const data = await apiService.getTeamStats(selected);
        setProfile(data);
      } catch {
        setProfile(null);
      } finally {
        setLoadingProfile(false);
      }
    })();
  }, [selected]);

  // Group teams by league
  const grouped = useMemo(() => {
    const filtered = search
      ? teams.filter(t => t.name.toLowerCase().includes(search.toLowerCase()))
      : teams;
    const map: Record<string, Team[]> = {};
    filtered.forEach(t => {
      if (!map[t.league]) map[t.league] = [];
      map[t.league].push(t);
    });
    return map;
  }, [teams, search]);

  const eloChartData = profile?.elo_history?.map(p => ({
    date: format(new Date(p.date), 'MMM yy'),
    elo: Math.round(p.elo_rating),
  })) ?? [];

  const team   = profile?.team;
  const stats  = profile?.current_stats;
  const recent = profile?.recent_matches ?? [];

  const winRate = stats && stats.matches_played > 0
    ? ((stats.wins / stats.matches_played) * 100).toFixed(1)
    : '0.0';

  const ppg = stats && stats.matches_played > 0
    ? (((stats.wins * 3) + (stats.draws ?? 0)) / stats.matches_played).toFixed(2)
    : '0.00';

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <Users size={32} />
          <div>
            <h1>Team Profiles</h1>
            <p>ELO ratings, recent form, and season statistics</p>
          </div>
        </div>
      </div>

      {error && (
        <div className="error-banner">
          <AlertCircle size={18} />
          <span>{error}</span>
        </div>
      )}

      <div className="teams-layout">
        {/* ── Sidebar ── */}
        <aside className="teams-sidebar">
          <div className="sidebar-header">
            <h3>Teams</h3>
            <input
              className="sidebar-search"
              placeholder="Search teams..."
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
          </div>

          {loadingTeams ? (
            <div className="loading" style={{ padding: '40px 20px' }}>
              <RefreshCw className="spin" size={24} />
            </div>
          ) : (
            Object.entries(grouped).map(([league, leagueTeams]) => (
              <div key={league} className="league-section">
                <span className="league-label">{league}</span>
                {leagueTeams.map(t => (
                  <button
                    key={t.id}
                    className={`team-item ${selected === t.id ? 'team-item-active' : ''}`}
                    onClick={() => setSelected(t.id)}
                  >
                    <span className="team-item-name">{t.name}</span>
                    <span className="team-item-elo">{Math.round(t.elo_rating)}</span>
                  </button>
                ))}
              </div>
            ))
          )}
        </aside>

        {/* ── Main content ── */}
        <div>
          {!selected ? (
            <div className="select-prompt">
              <Users size={48} />
              <h3>Select a team</h3>
              <p>Choose a team from the sidebar to view their profile</p>
            </div>
          ) : loadingProfile ? (
            <div className="loading">
              <RefreshCw className="spin" size={32} />
              <p>Loading team profile...</p>
            </div>
          ) : team ? (
            <div className="team-detail">
              {/* Hero */}
              <div className="team-hero">
                <div className="team-avatar">
                  {getEmoji(team.sport, team.league)}
                </div>
                <div className="team-hero-info">
                  <h2>{team.name}</h2>
                  <p>{team.league} · {team.sport === 'football' ? 'Football' : 'Basketball'}</p>
                  {stats && <p style={{ fontSize: '0.82rem', color: 'var(--text-muted)', marginTop: 4 }}>Season {stats.season}</p>}
                </div>
                <div className="elo-display">
                  <div className="elo-label">ELO Rating</div>
                  <div className="elo-value">{Math.round(team.elo_rating)}</div>
                </div>
              </div>

              {/* Key stats */}
              {stats && (
                <div className="team-stats-row">
                  <div className="team-stat-card">
                    <div className="stat-value">{stats.matches_played}</div>
                    <div className="stat-label">Played</div>
                  </div>
                  <div className="team-stat-card">
                    <div className="stat-value" style={{ color: 'var(--green)' }}>{stats.wins}</div>
                    <div className="stat-label">Wins</div>
                  </div>
                  {stats.draws != null && (
                    <div className="team-stat-card">
                      <div className="stat-value" style={{ color: 'var(--yellow)' }}>{stats.draws}</div>
                      <div className="stat-label">Draws</div>
                    </div>
                  )}
                  <div className="team-stat-card">
                    <div className="stat-value" style={{ color: 'var(--red)' }}>{stats.losses}</div>
                    <div className="stat-label">Losses</div>
                  </div>
                  <div className="team-stat-card">
                    <div className="stat-value">{winRate}%</div>
                    <div className="stat-label">Win Rate</div>
                  </div>
                  {team.sport === 'football' ? (
                    <>
                      <div className="team-stat-card">
                        <div className="stat-value">{stats.goals_for ?? 0}</div>
                        <div className="stat-label">Goals For</div>
                      </div>
                      <div className="team-stat-card">
                        <div className="stat-value">{stats.goals_against ?? 0}</div>
                        <div className="stat-label">Goals Against</div>
                      </div>
                      <div className="team-stat-card">
                        <div className="stat-value">{ppg}</div>
                        <div className="stat-label">Pts / Game</div>
                      </div>
                    </>
                  ) : (
                    <>
                      <div className="team-stat-card">
                        <div className="stat-value">
                          {stats.points_for ? (stats.points_for / stats.matches_played).toFixed(1) : '0.0'}
                        </div>
                        <div className="stat-label">PPG (For)</div>
                      </div>
                      <div className="team-stat-card">
                        <div className="stat-value">
                          {stats.points_against ? (stats.points_against / stats.matches_played).toFixed(1) : '0.0'}
                        </div>
                        <div className="stat-label">PPG (Allowed)</div>
                      </div>
                    </>
                  )}
                </div>
              )}

              {/* Recent form */}
              {stats?.form && (
                <div className="chart-section">
                  <h3>Recent Form</h3>
                  <div className="form-badges">
                    {stats.form.split('').map((r, i) => (
                      <div
                        key={i}
                        className={`form-badge form-${r.toLowerCase()}`}
                        title={r === 'W' ? 'Win' : r === 'D' ? 'Draw' : 'Loss'}
                      >
                        {r}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* ELO History Chart */}
              {eloChartData.length > 0 && (
                <div className="chart-section">
                  <h3>ELO Rating History — 2025/26 Season</h3>
                  <ResponsiveContainer width="100%" height={220}>
                    <LineChart data={eloChartData} margin={{ top: 8, right: 16, left: 0, bottom: 0 }}>
                      <CartesianGrid strokeDasharray="3 3" />
                      <XAxis dataKey="date" tick={{ fontSize: 11 }} />
                      <YAxis
                        domain={['auto', 'auto']}
                        tick={{ fontSize: 11 }}
                        tickFormatter={v => v.toFixed(0)}
                        width={46}
                      />
                      <Tooltip content={<EloTooltip />} />
                      <Line
                        type="monotone"
                        dataKey="elo"
                        stroke="#6366f1"
                        strokeWidth={2.5}
                        dot={{ fill: '#6366f1', r: 4 }}
                        activeDot={{ r: 6, stroke: '#818cf8', strokeWidth: 2 }}
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>
              )}

              {/* Recent matches */}
              <div className="chart-section">
                <h3>Recent Results</h3>
                {recent.length === 0 ? (
                  <p className="no-data">No recent match data available</p>
                ) : (
                  <div className="recent-matches">
                    {recent.map(match => {
                      const result = getMatchResult(match, team.id);
                      const isHome  = match.home_team_id === team.id;
                      const opponent = isHome ? match.away_team_name : match.home_team_name;
                      const venue    = isHome ? 'vs' : '@';
                      const score    = match.home_score != null
                        ? `${match.home_score}–${match.away_score}`
                        : 'TBD';

                      return (
                        <div key={match.id} className="recent-match">
                          <span className="recent-match-date">
                            {format(new Date(match.match_date), 'MMM d')}
                          </span>
                          <span className="recent-match-teams">
                            {venue} <strong>{opponent}</strong>
                          </span>
                          <span className="recent-match-score">{score}</span>
                          {result && (
                            <span className={`recent-match-result result-${result.toLowerCase()}`}>
                              {result}
                            </span>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          ) : (
            <div className="select-prompt">
              <AlertCircle size={40} />
              <h3>Team not found</h3>
              <p>Try selecting another team from the sidebar</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default TeamProfile;
