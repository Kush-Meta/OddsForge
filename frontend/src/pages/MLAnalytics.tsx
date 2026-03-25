import React, { useEffect, useState, useCallback } from 'react';
import {
  LineChart, Line, BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip,
  ResponsiveContainer, ReferenceLine, Cell, ComposedChart, Area,
} from 'recharts';
import { Brain, TrendingUp, Database, Activity, RefreshCw, ChevronLeft, ChevronRight } from 'lucide-react';
import { mlApi, apiService, MlEvaluation, FeatureContribution, ScoreDistribution, HistoryGame } from '../services/api';

// ── helpers ────────────────────────────────────────────────────────────────────

const fmt = (n: number, dp = 4) => n.toFixed(dp);
const pct = (n: number, dp = 1) => `${(n * 100).toFixed(dp)}%`;

function foldToYear(fold: number): number {
  return fold + 2003; // dataset starts 2003; fold = eval_yr - min_yr
}

// ── sub-components ─────────────────────────────────────────────────────────────

const StatCard: React.FC<{
  icon: React.ReactNode;
  label: string;
  value: string;
  sub?: string;
  color: string;
}> = ({ icon, label, value, sub, color }) => (
  <div className="ml-stat-card" style={{ '--accent': color } as React.CSSProperties}>
    <div className="ml-stat-icon" style={{ color }}>{icon}</div>
    <div className="ml-stat-body">
      <div className="ml-stat-label">{label}</div>
      <div className="ml-stat-value" style={{ color }}>{value}</div>
      {sub && <div className="ml-stat-sub">{sub}</div>}
    </div>
  </div>
);

const SectionHeader: React.FC<{ title: string; sub: string }> = ({ title, sub }) => (
  <div className="ml-section-header">
    <h2 className="ml-section-title">{title}</h2>
    <p className="ml-section-sub">{sub}</p>
  </div>
);

// Custom tooltip for backtest chart
const BacktestTooltip: React.FC<any> = ({ active, payload, label }) => {
  if (!active || !payload?.length) return null;
  return (
    <div className="ml-tooltip">
      <div className="ml-tooltip-year">{label}</div>
      {payload.map((p: any) => (
        <div key={p.name} className="ml-tooltip-row" style={{ color: p.color }}>
          <span>{p.name}:</span>
          <span>{p.name === 'Accuracy' ? pct(p.value / 100) : fmt(p.value)}</span>
        </div>
      ))}
      {payload[0] && <div className="ml-tooltip-games">{payload[0].payload.n_games?.toLocaleString()} games</div>}
    </div>
  );
};

const FeatureTooltip: React.FC<any> = ({ active, payload }) => {
  if (!active || !payload?.length) return null;
  const d = payload[0].payload;
  return (
    <div className="ml-tooltip">
      <div className="ml-tooltip-year">{d.feature_name}</div>
      <div className="ml-tooltip-row" style={{ color: d.contribution >= 0 ? '#FF6B35' : '#00D4FF' }}>
        <span>Contribution:</span>
        <span>{d.contribution >= 0 ? '+' : ''}{fmt(d.contribution, 4)}</span>
      </div>
      <div className="ml-tooltip-games">Value: {fmt(d.feature_value, 3)}</div>
    </div>
  );
};

const DistTooltip: React.FC<any> = ({ active, payload, label }) => {
  if (!active || !payload?.length) return null;
  return (
    <div className="ml-tooltip">
      <div className="ml-tooltip-year">Margin: {label > 0 ? `+${label}` : label}</div>
      <div className="ml-tooltip-row" style={{ color: Number(label) > 0 ? '#FF6B35' : '#00D4FF' }}>
        <span>Probability:</span>
        <span>{pct(payload[0].value)}</span>
      </div>
    </div>
  );
};

// ── main page ──────────────────────────────────────────────────────────────────

const MLAnalytics: React.FC = () => {
  const [evals, setEvals] = useState<MlEvaluation[]>([]);
  const [features, setFeatures] = useState<FeatureContribution[]>([]);
  const [distribution, setDistribution] = useState<ScoreDistribution | null>(null);
  const [history, setHistory] = useState<HistoryGame[]>([]);
  const [historyPage, setHistoryPage] = useState(0);
  const [sampleMatchName, setSampleMatchName] = useState('');
  const [training, setTraining] = useState(false);
  const [loading, setLoading] = useState(true);
  const PAGE_SIZE = 20;

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [evalData, upcoming, hist] = await Promise.all([
        mlApi.getEvaluations(),
        apiService.getUpcomingMatches('basketball', 5),
        mlApi.getMatchHistory(200, 0),
      ]);
      setEvals(evalData);
      setHistory(hist);

      // Pick first upcoming NBA game for feature importance + distribution
      const first = upcoming[0];
      if (first) {
        setSampleMatchName(`${first.match_info.home_team_name} vs ${first.match_info.away_team_name}`);
        const [feat, dist] = await Promise.all([
          mlApi.getFeatureImportance(first.match_info.id).catch(() => []),
          first.prediction
            ? mlApi.getScoreDistribution(first.prediction.id).catch(() => null)
            : Promise.resolve(null),
        ]);
        setFeatures(feat);
        setDistribution(dist);
      }
    } catch (e) {
      console.error(e);
    }
    setLoading(false);
  }, []);

  useEffect(() => { load(); }, [load]);

  // ── derived metrics ──────────────────────────────────────────────────────────
  const avgBrier = evals.length ? evals.reduce((s, e) => s + e.brier_score, 0) / evals.length : 0;
  const avgAcc = evals.length ? evals.reduce((s, e) => s + e.accuracy, 0) / evals.length : 0;
  const totalGames = evals.reduce((s, e) => s + e.n_games, 0);
  const modelVersion = 'ml_v1.0';

  // Walk-forward chart data
  const backtestData = evals.map(e => ({
    year: foldToYear(e.fold),
    Accuracy: +(e.accuracy * 100).toFixed(1),
    Brier: +e.brier_score.toFixed(4),
    n_games: e.n_games,
  })).sort((a, b) => a.year - b.year);

  // Feature importance — top 12 sorted by absolute contribution
  const topFeatures = [...features]
    .sort((a, b) => Math.abs(b.contribution) - Math.abs(a.contribution))
    .slice(0, 12)
    .reverse(); // recharts horizontal bar renders bottom-up

  // Score distribution — collapse 80 buckets into meaningful range (-30..+30)
  const distData = distribution
    ? distribution.buckets
        .map((p, i) => ({ margin: i - 40, prob: p }))
        .filter(d => d.margin >= -30 && d.margin <= 30)
    : [];

  // History pagination
  const pageData = history.slice(historyPage * PAGE_SIZE, (historyPage + 1) * PAGE_SIZE);
  const totalPages = Math.ceil(history.length / PAGE_SIZE);

  const handleTrain = async () => {
    setTraining(true);
    await mlApi.triggerTrain().catch(console.error);
    setTimeout(() => { setTraining(false); load(); }, 2000);
  };

  if (loading) {
    return (
      <div className="ml-loading">
        <div className="ml-loading-spinner" />
        <p>Loading ML Analytics…</p>
      </div>
    );
  }

  return (
    <div className="ml-analytics">

      {/* ── Hero header ──────────────────────────────────────────────────────── */}
      <div className="ml-hero">
        <div className="ml-hero-left">
          <div className="ml-hero-badge">
            <Brain size={16} />
            <span>Stacked Ensemble Model</span>
          </div>
          <h1 className="ml-hero-title">ML Analytics</h1>
          <p className="ml-hero-sub">
            Four-model ensemble trained on <strong>27,152 NBA games</strong> (2003 – 2026).<br />
            Bayesian Poisson · RAPM Ridge · Gradient Boosted Trees · Monte Carlo Simulation
          </p>
        </div>
        <button
          className={`ml-train-btn ${training ? 'ml-train-btn-active' : ''}`}
          onClick={handleTrain}
          disabled={training}
        >
          <RefreshCw size={16} className={training ? 'spin' : ''} />
          {training ? 'Training…' : 'Retrain Model'}
        </button>
      </div>

      {/* ── Stat cards ───────────────────────────────────────────────────────── */}
      <div className="ml-stats-row">
        <StatCard
          icon={<Activity size={22} />}
          label="Avg Brier Score"
          value={fmt(avgBrier)}
          sub="Lower is better · random = 0.25"
          color="#FF6B35"
        />
        <StatCard
          icon={<TrendingUp size={22} />}
          label="Avg Accuracy"
          value={pct(avgAcc)}
          sub={`Best fold: ${pct(Math.max(...evals.map(e => e.accuracy)))}`}
          color="#10B981"
        />
        <StatCard
          icon={<Database size={22} />}
          label="Games Trained"
          value={totalGames.toLocaleString()}
          sub={`${evals.length} walk-forward folds`}
          color="#00D4FF"
        />
        <StatCard
          icon={<Brain size={22} />}
          label="Model Version"
          value={modelVersion}
          sub="Isotonic calibrated"
          color="#8B5CF6"
        />
      </div>

      {/* ── Walk-forward backtest chart ───────────────────────────────────────── */}
      <div className="ml-card ml-card-full">
        <SectionHeader
          title="Walk-Forward Backtest Performance"
          sub="Model evaluated on unseen seasons year-by-year — no look-ahead bias"
        />
        <div className="ml-chart-container" style={{ height: 320 }}>
          <ResponsiveContainer width="100%" height="100%">
            <ComposedChart data={backtestData} margin={{ top: 10, right: 40, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="accGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#FF6B35" stopOpacity={0.2} />
                  <stop offset="95%" stopColor="#FF6B35" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" />
              <XAxis
                dataKey="year"
                tick={{ fill: '#8896B3', fontSize: 12 }}
                axisLine={{ stroke: 'rgba(255,255,255,0.1)' }}
                tickLine={false}
              />
              <YAxis
                yAxisId="acc"
                domain={[45, 70]}
                tick={{ fill: '#FF6B35', fontSize: 12 }}
                axisLine={false}
                tickLine={false}
                tickFormatter={(v) => `${v}%`}
              />
              <YAxis
                yAxisId="brier"
                orientation="right"
                domain={[0.20, 0.28]}
                tick={{ fill: '#00D4FF', fontSize: 12 }}
                axisLine={false}
                tickLine={false}
                tickFormatter={(v) => v.toFixed(3)}
              />
              <Tooltip content={<BacktestTooltip />} />
              <ReferenceLine yAxisId="acc" y={50} stroke="rgba(255,255,255,0.15)" strokeDasharray="4 4" />
              <Area
                yAxisId="acc"
                type="monotone"
                dataKey="Accuracy"
                stroke="#FF6B35"
                strokeWidth={2.5}
                fill="url(#accGrad)"
                dot={{ r: 3, fill: '#FF6B35' }}
                activeDot={{ r: 6, fill: '#FF6B35', stroke: '#fff', strokeWidth: 2 }}
              />
              <Line
                yAxisId="brier"
                type="monotone"
                dataKey="Brier"
                stroke="#00D4FF"
                strokeWidth={2}
                dot={{ r: 3, fill: '#00D4FF' }}
                activeDot={{ r: 6, fill: '#00D4FF', stroke: '#fff', strokeWidth: 2 }}
                strokeDasharray="6 2"
              />
            </ComposedChart>
          </ResponsiveContainer>
        </div>
        <div className="ml-chart-legend">
          <span className="ml-legend-item" style={{ color: '#FF6B35' }}>
            <span className="ml-legend-dot" style={{ background: '#FF6B35' }} /> Accuracy % (left axis)
          </span>
          <span className="ml-legend-item" style={{ color: '#00D4FF' }}>
            <span className="ml-legend-dot" style={{ background: '#00D4FF', borderRadius: 0 }} /> Brier Score (right axis)
          </span>
          <span className="ml-legend-note">Dashed line = 50% baseline</span>
        </div>
      </div>

      {/* ── Feature importance + Score distribution ──────────────────────────── */}
      <div className="ml-two-col">

        {/* Feature importance */}
        <div className="ml-card">
          <SectionHeader
            title="Feature Importance"
            sub={sampleMatchName ? `Permutation importance — ${sampleMatchName}` : 'Permutation importance'}
          />
          {topFeatures.length === 0 ? (
            <div className="ml-empty">No ML model loaded. Train a model first.</div>
          ) : (
            <div className="ml-chart-container" style={{ height: 360 }}>
              <ResponsiveContainer width="100%" height="100%">
                <BarChart
                  layout="vertical"
                  data={topFeatures}
                  margin={{ top: 0, right: 20, left: 130, bottom: 0 }}
                >
                  <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" horizontal={false} />
                  <XAxis
                    type="number"
                    tick={{ fill: '#8896B3', fontSize: 11 }}
                    axisLine={false}
                    tickLine={false}
                    tickFormatter={(v) => v > 0 ? `+${v.toFixed(3)}` : v.toFixed(3)}
                  />
                  <YAxis
                    type="category"
                    dataKey="feature_name"
                    tick={{ fill: '#8896B3', fontSize: 11 }}
                    axisLine={false}
                    tickLine={false}
                    width={125}
                  />
                  <Tooltip content={<FeatureTooltip />} />
                  <ReferenceLine x={0} stroke="rgba(255,255,255,0.2)" />
                  <Bar dataKey="contribution" radius={[0, 3, 3, 0]}>
                    {topFeatures.map((f, i) => (
                      <Cell
                        key={i}
                        fill={f.contribution >= 0 ? '#FF6B35' : '#00D4FF'}
                        fillOpacity={0.85}
                      />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          )}
          <div className="ml-feature-legend">
            <span style={{ color: '#FF6B35' }}>■ Favours home win</span>
            <span style={{ color: '#00D4FF' }}>■ Favours away win</span>
          </div>
        </div>

        {/* Score distribution */}
        <div className="ml-card">
          <SectionHeader
            title="Score Distribution"
            sub={distribution ? `Monte Carlo (5,000 sims) — ${sampleMatchName}` : 'Monte Carlo margin histogram'}
          />
          {!distribution || distData.length === 0 ? (
            <div className="ml-empty">Score distribution requires a trained ML model.</div>
          ) : (
            <>
              <div className="ml-dist-summary">
                <div className="ml-dist-pill" style={{ background: 'rgba(255,107,53,0.15)', color: '#FF6B35' }}>
                  Home win: {pct(distribution.p_home_win)}
                </div>
                <div className="ml-dist-pill" style={{ background: 'rgba(0,212,255,0.15)', color: '#00D4FF' }}>
                  Away win: {pct(1 - distribution.p_home_win)}
                </div>
                <div className="ml-dist-pill" style={{ background: 'rgba(139,92,246,0.15)', color: '#8B5CF6' }}>
                  Exp. margin: {distribution.expected_margin > 0 ? '+' : ''}{distribution.expected_margin.toFixed(1)}
                </div>
              </div>
              <div className="ml-chart-container" style={{ height: 290 }}>
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={distData} margin={{ top: 5, right: 10, left: -20, bottom: 0 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" vertical={false} />
                    <XAxis
                      dataKey="margin"
                      tick={{ fill: '#8896B3', fontSize: 11 }}
                      axisLine={false}
                      tickLine={false}
                      tickFormatter={(v) => v === 0 ? '0' : v > 0 ? `+${v}` : `${v}`}
                      interval={4}
                    />
                    <YAxis
                      tick={{ fill: '#8896B3', fontSize: 11 }}
                      axisLine={false}
                      tickLine={false}
                      tickFormatter={(v) => pct(v, 0)}
                    />
                    <Tooltip content={<DistTooltip />} />
                    <ReferenceLine x={0} stroke="rgba(255,255,255,0.3)" strokeWidth={1.5} />
                    <Bar dataKey="prob" radius={[2, 2, 0, 0]}>
                      {distData.map((d, i) => (
                        <Cell key={i} fill={d.margin > 0 ? '#FF6B35' : '#00D4FF'} fillOpacity={0.7 + 0.3 * Math.abs(d.margin) / 30} />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
              <p className="ml-dist-note">Margin = home score − away score. Orange = home wins.</p>
            </>
          )}
        </div>

      </div>

      {/* ── Model architecture cards ──────────────────────────────────────────── */}
      <div className="ml-card ml-card-full">
        <SectionHeader
          title="Ensemble Architecture"
          sub="Four specialised models stacked via logistic meta-learner with isotonic calibration"
        />
        <div className="ml-arch-grid">
          {[
            {
              icon: '🎲',
              name: 'Bayesian Poisson',
              tag: 'Dixon-Coles',
              color: '#FF6B35',
              desc: 'Each team has latent attack α and defence δ parameters. Score modelled as Poisson(α_i × δ_j × μ × HCA). Parameters estimated by MLE gradient descent over pace-adjusted pts/100 possessions.',
            },
            {
              icon: '📐',
              name: 'RAPM Ridge',
              tag: 'Regularised Adjusted +/−',
              color: '#8B5CF6',
              desc: 'Ridge regression on the game matrix (+1 home, \u22121 away). Target: point differential. Lambda tuned to \u03bb=5. Coefficients represent each team\u2019s true contribution above/below average.',
            },
            {
              icon: '🌲',
              name: 'Gradient Boosted Trees',
              tag: '150-stump additive ensemble',
              color: '#10B981',
              desc: '26-feature matchup vectors → binary win/loss. Each stump fits the negative gradient of log-loss. Features include ELO, Four Factors, rest days, H2H rate, and opponent-adjusted form.',
            },
            {
              icon: '🎯',
              name: 'Monte Carlo Sim',
              tag: 'Markov chain possessions',
              color: '#00D4FF',
              desc: '10,000 games simulated as Markov chains over 5 possession states. Transition probabilities driven by team Four Factors. Returns full margin distribution, not just a point estimate.',
            },
          ].map(m => (
            <div className="ml-arch-card" key={m.name} style={{ '--accent': m.color } as React.CSSProperties}>
              <div className="ml-arch-icon">{m.icon}</div>
              <div className="ml-arch-content">
                <div className="ml-arch-name" style={{ color: m.color }}>{m.name}</div>
                <div className="ml-arch-tag">{m.tag}</div>
                <p className="ml-arch-desc">{m.desc}</p>
              </div>
            </div>
          ))}
        </div>
        <div className="ml-arch-meta">
          <div className="ml-arch-meta-item">
            <span className="ml-arch-meta-label">Meta-learner</span>
            <span>Logistic regression over 8 inputs (4 base probs + ELO diff + form diff + net rating diff + H2H rate)</span>
          </div>
          <div className="ml-arch-meta-item">
            <span className="ml-arch-meta-label">Calibration</span>
            <span>Isotonic regression via Pool Adjacent Violators — maps raw scores to well-calibrated probabilities</span>
          </div>
          <div className="ml-arch-meta-item">
            <span className="ml-arch-meta-label">Validation</span>
            <span>Walk-forward annual folds; each year evaluated on data the model never saw during training</span>
          </div>
        </div>
      </div>

      {/* ── Historical games table ────────────────────────────────────────────── */}
      <div className="ml-card ml-card-full">
        <div className="ml-section-header-row">
          <SectionHeader
            title="Historical Game Record"
            sub={`${history.length.toLocaleString()} finished NBA games · Kaggle dataset 2003–2026`}
          />
          <div className="ml-pagination">
            <button
              className="ml-page-btn"
              onClick={() => setHistoryPage(p => Math.max(0, p - 1))}
              disabled={historyPage === 0}
            >
              <ChevronLeft size={16} />
            </button>
            <span className="ml-page-info">{historyPage + 1} / {totalPages}</span>
            <button
              className="ml-page-btn"
              onClick={() => setHistoryPage(p => Math.min(totalPages - 1, p + 1))}
              disabled={historyPage >= totalPages - 1}
            >
              <ChevronRight size={16} />
            </button>
          </div>
        </div>

        <div className="ml-table-wrap">
          <table className="ml-table">
            <thead>
              <tr>
                <th>Date</th>
                <th>Season</th>
                <th>Home Team</th>
                <th>Score</th>
                <th>Away Team</th>
                <th>Result</th>
              </tr>
            </thead>
            <tbody>
              {pageData.map((g) => (
                <tr key={g.match_id} className={g.home_won ? 'ml-row-home-win' : 'ml-row-away-win'}>
                  <td className="ml-td-date">
                    {new Date(g.match_date).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}
                  </td>
                  <td className="ml-td-season">{g.season}/{String(g.season + 1).slice(-2)}</td>
                  <td className="ml-td-team ml-td-home">
                    <span className={g.home_won ? 'ml-team-winner' : ''}>{g.home_team}</span>
                  </td>
                  <td className="ml-td-score">
                    <span className={g.home_won ? 'ml-score-winner' : ''}>{g.home_score}</span>
                    <span className="ml-score-sep">—</span>
                    <span className={!g.home_won ? 'ml-score-winner' : ''}>{g.away_score}</span>
                  </td>
                  <td className="ml-td-team ml-td-away">
                    <span className={!g.home_won ? 'ml-team-winner' : ''}>{g.away_team}</span>
                  </td>
                  <td>
                    <span className={`ml-result-badge ${g.home_won ? 'ml-result-home' : 'ml-result-away'}`}>
                      {g.home_won ? 'HOME' : 'AWAY'}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

    </div>
  );
};

export default MLAnalytics;
