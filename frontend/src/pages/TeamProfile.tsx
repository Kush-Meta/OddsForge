import React, { useState, useEffect } from 'react';
import { Users, TrendingUp, Calendar, Trophy, Search, ArrowLeft } from 'lucide-react';
import { apiService, Team } from '../services/api';

const TeamProfile: React.FC = () => {
  const [teams, setTeams] = useState<Team[]>([]);
  const [selectedTeam, setSelectedTeam] = useState<Team | null>(null);
  const [loading, setLoading] = useState(true);
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedSport, setSelectedSport] = useState<string>('all');

  const fetchTeams = async () => {
    setLoading(true);
    try {
      const footballTeams = await apiService.getTeamsByLeague('football', 'EPL');
      const basketballTeams = await apiService.getTeamsByLeague('basketball', 'NBA');
      setTeams([...footballTeams, ...basketballTeams]);
    } catch (err) {
      console.error('Error fetching teams:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchTeams();
  }, []);

  const filteredTeams = teams.filter(team => {
    const matchesSearch = team.name.toLowerCase().includes(searchTerm.toLowerCase());
    const matchesSport = selectedSport === 'all' || team.sport === selectedSport;
    return matchesSearch && matchesSport;
  });

  const getLeagueIcon = (sport: string, league: string) => {
    if (sport === 'football') return '⚽';
    if (sport === 'basketball') return '🏀';
    return '🏟️';
  };

  const getEloColor = (rating: number) => {
    if (rating > 1400) return 'elo-high';
    if (rating > 1250) return 'elo-medium';
    return 'elo-low';
  };

  if (selectedTeam) {
    return (
      <div className="page">
        <div className="page-header">
          <div className="page-title">
            <button 
              onClick={() => setSelectedTeam(null)}
              className="back-btn"
            >
              <ArrowLeft size={20} />
            </button>
            <div>
              <h1>{selectedTeam.name}</h1>
              <p>{selectedTeam.league} • {selectedTeam.sport}</p>
            </div>
          </div>
          <div className={`elo-badge ${getEloColor(selectedTeam.elo_rating)}`}>
            {selectedTeam.elo_rating.toFixed(0)} ELO
          </div>
        </div>

        <div className="team-profile">
          <div className="team-stats-grid">
            <div className="stat-card">
              <div className="stat-icon">{getLeagueIcon(selectedTeam.sport, selectedTeam.league)}</div>
              <div>
                <div className="stat-value">{selectedTeam.league}</div>
                <div className="stat-label">League</div>
              </div>
            </div>
            
            <div className="stat-card">
              <div className="stat-icon">📊</div>
              <div>
                <div className="stat-value">{selectedTeam.elo_rating.toFixed(0)}</div>
                <div className="stat-label">ELO Rating</div>
              </div>
            </div>
            
            <div className="stat-card">
              <div className="stat-icon">📈</div>
              <div>
                <div className="stat-value">
                  {selectedTeam.elo_rating > 1200 ? '+' : ''}{(selectedTeam.elo_rating - 1200).toFixed(0)}
                </div>
                <div className="stat-label">vs Average</div>
              </div>
            </div>
          </div>

          <div className="team-sections">
            <div className="team-section">
              <h3><Calendar size={20} /> Recent Matches</h3>
              <div className="matches-list">
                <div className="no-data">
                  <Calendar size={32} />
                  <p>No recent match data available</p>
                  <small>Match history will appear here once games are played</small>
                </div>
              </div>
            </div>

            <div className="team-section">
              <h3><TrendingUp size={20} /> ELO History</h3>
              <div className="chart-container">
                <div className="elo-trend">
                  <div className="trend-point current">
                    <span className="trend-date">Current</span>
                    <span className="trend-value">{selectedTeam.elo_rating.toFixed(0)}</span>
                  </div>
                </div>
                <div className="no-data">
                  <TrendingUp size={32} />
                  <p>Historical ELO chart will be shown here</p>
                  <small>Requires charting library integration</small>
                </div>
              </div>
            </div>

            <div className="team-section">
              <h3><Trophy size={20} /> Season Stats</h3>
              <div className="stats-table">
                <div className="stat-row">
                  <span>Matches Played</span>
                  <span>0</span>
                </div>
                <div className="stat-row">
                  <span>Win Rate</span>
                  <span>--%</span>
                </div>
                <div className="stat-row">
                  <span>Goals/Points Per Game</span>
                  <span>--</span>
                </div>
                <div className="stat-row">
                  <span>Last Updated</span>
                  <span>{new Date(selectedTeam.updated_at).toLocaleDateString()}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-header">
        <div className="page-title">
          <Users size={32} />
          <div>
            <h1>Team Profiles</h1>
            <p>Browse teams and view detailed statistics</p>
          </div>
        </div>
        
        <div className="page-actions">
          <div className="search-container">
            <Search size={18} />
            <input
              type="text"
              placeholder="Search teams..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="search-input"
            />
          </div>
          <select 
            value={selectedSport} 
            onChange={(e) => setSelectedSport(e.target.value)}
            className="sport-filter"
          >
            <option value="all">All Sports</option>
            <option value="football">Football</option>
            <option value="basketball">Basketball</option>
          </select>
        </div>
      </div>

      {loading ? (
        <div className="loading">
          <div className="spinner" />
          <p>Loading teams...</p>
        </div>
      ) : (
        <div className="teams-grid">
          {filteredTeams.map(team => (
            <div 
              key={team.id} 
              className="team-card"
              onClick={() => setSelectedTeam(team)}
            >
              <div className="team-card-header">
                <div className="team-info">
                  <div className="team-icon">
                    {getLeagueIcon(team.sport, team.league)}
                  </div>
                  <div>
                    <h3 className="team-name">{team.name}</h3>
                    <p className="team-league">{team.league}</p>
                  </div>
                </div>
                <div className={`elo-badge ${getEloColor(team.elo_rating)}`}>
                  {team.elo_rating.toFixed(0)}
                </div>
              </div>
              
              <div className="team-card-stats">
                <div className="quick-stat">
                  <span className="stat-label">ELO Rating</span>
                  <span className="stat-value">{team.elo_rating.toFixed(0)}</span>
                </div>
                <div className="quick-stat">
                  <span className="stat-label">vs Average</span>
                  <span className={`stat-value ${team.elo_rating > 1200 ? 'positive' : 'negative'}`}>
                    {team.elo_rating > 1200 ? '+' : ''}{(team.elo_rating - 1200).toFixed(0)}
                  </span>
                </div>
              </div>

              <div className="team-card-footer">
                <span className="view-profile">View Profile →</span>
              </div>
            </div>
          ))}
          
          {filteredTeams.length === 0 && !loading && (
            <div className="empty-state">
              <Search size={48} />
              <h3>No teams found</h3>
              <p>Try adjusting your search or filter criteria</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default TeamProfile;