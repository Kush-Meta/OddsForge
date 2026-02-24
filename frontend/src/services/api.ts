import axios from 'axios';

const API_BASE_URL = process.env.REACT_APP_API_URL || 'http://localhost:3000';

const api = axios.create({
  baseURL: API_BASE_URL,
  timeout: 10000,
});

// Types
export interface Team {
  id: string;
  name: string;
  sport: string;
  league: string;
  logo_url?: string;
  elo_rating: number;
  created_at: string;
  updated_at: string;
}

export interface Match {
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
  created_at: string;
  updated_at: string;
}

export interface Prediction {
  id: string;
  match_id: string;
  home_win_probability: number;
  away_win_probability: number;
  draw_probability?: number;
  model_version: string;
  confidence_score: number;
  created_at: string;
}

export interface UpcomingMatchWithPrediction {
  match_info: Match;
  prediction?: Prediction;
  home_team_stats?: any;
  away_team_stats?: any;
}

export interface Edge {
  match_id: string;
  match_info: Match;
  our_prediction: Prediction;
  market_home_odds: number;
  market_away_odds: number;
  market_draw_odds?: number;
  edge_value: number;
}

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
  timestamp: string;
}

// API Functions
export const apiService = {
  // Health check
  async healthCheck(): Promise<string> {
    const response = await api.get<ApiResponse<string>>('/health');
    return response.data.data || 'OK';
  },

  // All teams
  async getAllTeams(): Promise<Team[]> {
    const response = await api.get<ApiResponse<Team[]>>('/teams');
    return response.data.data || [];
  },

  // Matches
  async getUpcomingMatches(sport?: string, limit?: number): Promise<UpcomingMatchWithPrediction[]> {
    const params = new URLSearchParams();
    if (sport) params.append('sport', sport);
    if (limit) params.append('limit', limit.toString());
    
    const response = await api.get<ApiResponse<UpcomingMatchWithPrediction[]>>(`/matches/upcoming?${params}`);
    return response.data.data || [];
  },

  // Teams
  async getTeamStats(teamId: string): Promise<any> {
    const response = await api.get<ApiResponse<any>>(`/teams/${teamId}/stats`);
    return response.data.data;
  },

  async getTeamsByLeague(sport: string, league: string): Promise<Team[]> {
    const response = await api.get<ApiResponse<Team[]>>(`/teams/league/${sport}/${league}`);
    return response.data.data || [];
  },

  // Predictions
  async getPredictionEdges(): Promise<Edge[]> {
    const response = await api.get<ApiResponse<Edge[]>>('/predictions/edges');
    return response.data.data || [];
  },

  async generatePredictions(): Promise<string> {
    const response = await api.post<ApiResponse<string>>('/predictions/generate');
    return response.data.data || 'Success';
  },

  // Data management
  async fetchData(sport?: string): Promise<string> {
    const response = await api.post<ApiResponse<string>>('/data/fetch', { sport });
    return response.data.data || 'Success';
  },

  // Dataset generation
  async generateDataset(request: {
    sport: string;
    teams?: string[];
    date_from?: string;
    date_to?: string;
    stats_categories: string[];
    format: string;
  }): Promise<any> {
    const response = await api.post<ApiResponse<any>>('/datasets/generate', request);
    return response.data.data;
  },
};

export default apiService;