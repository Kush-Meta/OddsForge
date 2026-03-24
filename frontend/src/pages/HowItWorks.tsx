import React from 'react';
import { Link } from 'react-router-dom';
import { ArrowLeft } from 'lucide-react';

interface ModelBlock {
  icon: string;
  title: string;
  subtitle: string;
  color: string;
  glow: string;
  weight: string;
  description: string;
  details: string[];
}

const BLOCKS: ModelBlock[] = [
  {
    icon: '📈',
    title: 'ELO Rating System',
    subtitle: 'Strength of schedule adjusted',
    color: '#FF6B35',
    glow: 'rgba(255,107,53,0.3)',
    weight: '15–30%',
    description: 'Each team carries an ELO score that rises after wins and falls after losses. The margin of victory is rewarded logarithmically — a blowout moves the needle more than a close win, but with diminishing returns.',
    details: [
      'NBA K-factor: 20, Football K-factor: 32',
      'Home court advantage: +75 ELO points for NBA, +100 for football',
      'MOV multiplier: ln(1 + margin) × 0.45, capped at 2.5×',
      'Full season replay on every background refresh',
    ],
  },
  {
    icon: '🔬',
    title: 'NBA Advanced Stats',
    subtitle: 'Four Factors model with Bayesian shrinkage',
    color: '#8B5CF6',
    glow: 'rgba(139,92,246,0.3)',
    weight: '5–20%',
    description: 'Dean Oliver\'s Four Factors (eFG%, turnover rate, offensive rebounding, and free throw rate) — both offensive and defensive sides — give a possession-efficiency picture that raw scoring misses.',
    details: [
      'Off/Def rating pulled from stats.nba.com every 6 hours',
      'Bayesian shrinkage: credibility = min(games / 55, 1.0)',
      'Early season: regressed toward league average (0)',
      'Late season: full observed net rating used',
    ],
  },
  {
    icon: '🔥',
    title: 'Opponent-Adjusted Form',
    subtitle: 'Rolling 8-game exponential decay',
    color: '#F59E0B',
    glow: 'rgba(245,158,11,0.3)',
    weight: '25%',
    description: 'A win by 30 against a bad team counts for less than a 5-point win against an elite opponent. Each of the last 8 games is weighted by a decay factor and adjusted by the opponent\'s net rating.',
    details: [
      'adj_margin = raw_margin – (opponent_net_rating / 3.0)',
      'Decay weight: 0.85^n where n = games ago',
      'Minimum 3 games required; otherwise falls back to ELO',
      'Exponential decay ensures very recent form dominates',
    ],
  },
  {
    icon: '⚔️',
    title: 'Head-to-Head History',
    subtitle: 'Last 10 direct matchups',
    color: '#00D4FF',
    glow: 'rgba(0,212,255,0.3)',
    weight: '5–25%',
    description: 'Some teams simply have the other\'s number. H2H captures psychological and stylistic matchup dynamics that form and ELO can miss, particularly in playoff-style scenarios.',
    details: [
      'Maximum 10 most recent H2H games considered',
      'Weight declines from 25% early season to 5% late season',
      'Only counted when ≥ 3 H2H games exist in the DB',
      'Falls back to 50/50 with 0% weight if insufficient history',
    ],
  },
  {
    icon: '🗓️',
    title: 'Schedule & Rest',
    subtitle: 'Back-to-back, road trips, rest days',
    color: '#10B981',
    glow: 'rgba(16,185,129,0.3)',
    weight: 'Adjustment',
    description: 'An NBA team playing their second game in two nights, on the road, after three straight away games faces a real disadvantage. The schedule model quantifies this as a probability adjustment.',
    details: [
      'Back-to-back penalty: –3% probability for the fatigued team',
      'Home rest advantage: +2% when home team has 2+ more rest days',
      'Road trip penalty: –1.5% per game into a consecutive road trip (≥3)',
      'Applied as a flat offset after ensemble combination',
    ],
  },
];

const WeightDiagram: React.FC = () => (
  <div style={{
    background: 'rgba(255,255,255,0.03)',
    backdropFilter: 'blur(20px)',
    border: '1px solid rgba(255,255,255,0.08)',
    borderRadius: 16,
    padding: '24px',
    marginBottom: 32,
  }}>
    <div style={{ fontSize: '0.72rem', fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.1em', marginBottom: 16 }}>
      Dynamic Weight Distribution
    </div>

    {/* Early season */}
    <div style={{ marginBottom: 16 }}>
      <div style={{ fontSize: '0.75rem', color: 'var(--text-secondary)', marginBottom: 8, fontWeight: 600 }}>Early Season (&lt;20 games)</div>
      <div style={{ display: 'flex', height: 24, borderRadius: 6, overflow: 'hidden', gap: 2 }}>
        {[
          { label: 'ELO 30%', pct: 30, color: '#FF6B35' },
          { label: 'Form 25%', pct: 25, color: '#F59E0B' },
          { label: 'H2H 25%', pct: 25, color: '#00D4FF' },
          { label: 'Net 15%', pct: 15, color: '#8B5CF6' },
          { label: 'FF 5%', pct: 5, color: '#10B981' },
        ].map(s => (
          <div key={s.label} style={{ flex: s.pct, background: s.color, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '0.6rem', fontWeight: 700, color: 'rgba(0,0,0,0.7)', whiteSpace: 'nowrap', overflow: 'hidden', padding: '0 4px' }}>
            {s.pct >= 10 ? s.label : ''}
          </div>
        ))}
      </div>
    </div>

    {/* Late season */}
    <div>
      <div style={{ fontSize: '0.75rem', color: 'var(--text-secondary)', marginBottom: 8, fontWeight: 600 }}>Late Season (55+ games)</div>
      <div style={{ display: 'flex', height: 24, borderRadius: 6, overflow: 'hidden', gap: 2 }}>
        {[
          { label: 'ELO 15%', pct: 15, color: '#FF6B35' },
          { label: 'Form 25%', pct: 25, color: '#F59E0B' },
          { label: 'H2H 5%', pct: 5, color: '#00D4FF' },
          { label: 'Net 35%', pct: 35, color: '#8B5CF6' },
          { label: 'FF 20%', pct: 20, color: '#10B981' },
        ].map(s => (
          <div key={s.label} style={{ flex: s.pct, background: s.color, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '0.6rem', fontWeight: 700, color: 'rgba(0,0,0,0.7)', whiteSpace: 'nowrap', overflow: 'hidden', padding: '0 4px' }}>
            {s.pct >= 10 ? s.label : ''}
          </div>
        ))}
      </div>
    </div>

    <div style={{ marginTop: 12, fontSize: '0.7rem', color: 'var(--text-muted)' }}>
      Weights shift linearly as teams accumulate games. ELO dominates early (less data); net rating + four factors dominate late (more reliable stats).
    </div>
  </div>
);

const HowItWorks: React.FC = () => (
  <div className="page">
    {/* Header */}
    <div className="page-header">
      <div className="page-title">
        <div className="page-title-icon" style={{ fontSize: '1.4rem' }}>🏀</div>
        <div>
          <h1>How It Works</h1>
          <p>The prediction engine — every signal, every weight, explained</p>
        </div>
      </div>
      <Link to="/" style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '9px 16px', borderRadius: 8, background: 'var(--bg-glass-md)', border: '1px solid var(--border)', color: 'var(--text-secondary)', fontSize: '0.875rem', fontWeight: 600, textDecoration: 'none', transition: 'all 0.2s' }}>
        <ArrowLeft size={15} />
        Back to Dashboard
      </Link>
    </div>

    {/* Intro */}
    <div style={{
      background: 'linear-gradient(135deg, rgba(255,107,53,0.08) 0%, rgba(139,92,246,0.06) 100%)',
      border: '1px solid rgba(255,107,53,0.2)',
      borderRadius: 16,
      padding: '24px 28px',
      marginBottom: 32,
    }}>
      <div style={{ fontSize: '1rem', fontWeight: 700, color: 'var(--text-primary)', marginBottom: 8 }}>
        A multi-signal ensemble model, updated every 60 seconds
      </div>
      <div style={{ fontSize: '0.875rem', color: 'var(--text-secondary)', lineHeight: 1.7, maxWidth: 720 }}>
        OddsForge combines five independent signals into a single probability estimate. No single signal is trusted blindly — they are weighted dynamically based on how many games each team has played, so the model degrades gracefully at the start of a season when data is thin.
      </div>
    </div>

    {/* Weight diagram */}
    <WeightDiagram />

    {/* Model blocks */}
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))', gap: 16, marginBottom: 32 }}>
      {BLOCKS.map(block => (
        <div
          key={block.title}
          style={{
            background: 'rgba(255,255,255,0.03)',
            backdropFilter: 'blur(20px)',
            border: '1px solid rgba(255,255,255,0.08)',
            borderRadius: 16,
            padding: '22px',
            transition: 'all 0.25s ease',
            cursor: 'default',
            position: 'relative',
            overflow: 'hidden',
          }}
          onMouseEnter={e => {
            (e.currentTarget as HTMLDivElement).style.borderColor = `${block.color}50`;
            (e.currentTarget as HTMLDivElement).style.boxShadow = `0 0 40px ${block.glow}`;
            (e.currentTarget as HTMLDivElement).style.transform = 'translateY(-3px)';
          }}
          onMouseLeave={e => {
            (e.currentTarget as HTMLDivElement).style.borderColor = 'rgba(255,255,255,0.08)';
            (e.currentTarget as HTMLDivElement).style.boxShadow = 'none';
            (e.currentTarget as HTMLDivElement).style.transform = 'translateY(0)';
          }}
        >
          {/* Accent gradient top */}
          <div style={{ position: 'absolute', top: 0, left: 0, right: 0, height: 2, background: `linear-gradient(90deg, ${block.color}, transparent)` }} />

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 14 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <div style={{ width: 40, height: 40, borderRadius: 10, background: `${block.color}20`, border: `1px solid ${block.color}40`, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '1.2rem' }}>
                {block.icon}
              </div>
              <div>
                <div style={{ fontSize: '0.9rem', fontWeight: 700, color: 'var(--text-primary)' }}>{block.title}</div>
                <div style={{ fontSize: '0.72rem', color: 'var(--text-muted)', marginTop: 1 }}>{block.subtitle}</div>
              </div>
            </div>
            <div style={{ padding: '3px 10px', borderRadius: 999, background: `${block.color}15`, border: `1px solid ${block.color}40`, fontSize: '0.7rem', fontWeight: 700, color: block.color, whiteSpace: 'nowrap' }}>
              {block.weight}
            </div>
          </div>

          <p style={{ fontSize: '0.82rem', color: 'var(--text-secondary)', lineHeight: 1.6, marginBottom: 14 }}>
            {block.description}
          </p>

          <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
            {block.details.map((d, i) => (
              <div key={i} style={{ display: 'flex', gap: 8, fontSize: '0.75rem', color: 'var(--text-muted)' }}>
                <span style={{ color: block.color, flexShrink: 0 }}>→</span>
                <span>{d}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>

    {/* Final combination */}
    <div style={{
      background: 'rgba(255,255,255,0.03)',
      backdropFilter: 'blur(20px)',
      border: '1px solid rgba(255,255,255,0.08)',
      borderRadius: 16,
      padding: '24px 28px',
    }}>
      <div style={{ fontSize: '0.72rem', fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.1em', marginBottom: 12 }}>Final Output</div>
      <div style={{ fontFamily: 'monospace', fontSize: '0.85rem', color: 'var(--orange-bright)', background: 'rgba(0,0,0,0.3)', padding: '14px 18px', borderRadius: 8, marginBottom: 14, lineHeight: 1.8 }}>
        p_home = (w_elo × elo_prob) + (w_nr × nr_prob) + (w_ff × ff_prob)<br />
        &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;+ (w_form × form_prob) + (w_h2h × h2h_prob)<br />
        p_home = clamp(p_home + schedule_adjustment, 0.02, 0.98)
      </div>
      <div style={{ fontSize: '0.82rem', color: 'var(--text-secondary)', lineHeight: 1.7 }}>
        The raw combined probability is clamped to [2%, 98%] to avoid overconfident predictions. The confidence score is 1 − 2 × |p_home − 0.5|, so a 70%/30% prediction has a confidence of 40%, while an 80%/20% prediction has 60%.
      </div>
    </div>
  </div>
);

export default HowItWorks;
