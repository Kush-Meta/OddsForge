import React from 'react';
import { MatchAnalysis } from '../services/api';

interface Props {
  analysis: MatchAnalysis;
  homeName: string;
  awayName: string;
}

interface ComponentBarProps {
  homeProb: number;
  weight: number;
  narrative: string;
  label: string;
  icon: string;
  iconBg: string;
  homeName: string;
  awayName: string;
}

const ComponentBar: React.FC<ComponentBarProps> = ({
  homeProb, weight, narrative, label, icon, iconBg, homeName, awayName
}) => {
  const awayProb = 1 - homeProb;
  const homeW = Math.round(homeProb * 100);
  const awayW = Math.round(awayProb * 100);
  const homeWins = homeProb > 0.5;

  return (
    <div className="component-row">
      <div className="component-header">
        <div className="component-label">
          <div className="component-icon" style={{ background: iconBg }}>
            {icon}
          </div>
          {label}
        </div>
        <span className="component-weight">Weight {(weight * 100).toFixed(0)}%</span>
      </div>

      <div className="component-bar-track">
        <div
          className="component-bar-home"
          style={{ width: `${homeW}%` }}
        />
        <div
          className="component-bar-away"
          style={{ width: `${awayW}%` }}
        />
      </div>

      <div className="component-probs">
        <span className="component-prob-home" style={{ fontWeight: homeWins ? 700 : 400 }}>
          {homeName} {homeW}%
        </span>
        <span className="component-prob-away" style={{ fontWeight: !homeWins ? 700 : 400 }}>
          {awayW}% {awayName}
        </span>
      </div>

      <div className="component-narrative">{narrative}</div>
    </div>
  );
};

const AlgorithmBreakdown: React.FC<Props> = ({ analysis, homeName, awayName }) => {
  const { elo, form, h2h, schedule } = analysis;

  const scheduleWarnings = [
    analysis.schedule.home_on_back_to_back && { label: `${homeName} B2B`, warning: true },
    analysis.schedule.away_on_back_to_back && { label: `${awayName} B2B`, warning: true },
    analysis.schedule.away_consecutive_road >= 3 && { label: `${awayName} road trip (${schedule.away_consecutive_road})`, warning: true },
    !schedule.home_on_back_to_back && { label: `${homeName} rested (${schedule.home_rest_days}d)`, warning: false },
    !schedule.away_on_back_to_back && { label: `${awayName} rested (${schedule.away_rest_days}d)`, warning: false },
  ].filter(Boolean) as Array<{ label: string; warning: boolean }>;

  return (
    <div className="breakdown-inner">
      <div className="breakdown-title">
        <span>Algorithm Breakdown</span>
        <div className="breakdown-title-line" />
        <span style={{ whiteSpace: 'nowrap', color: 'var(--text-muted)' }}>
          v{analysis.model_version}
        </span>
      </div>

      <ComponentBar
        label="ELO Rating"
        icon="📈"
        iconBg="rgba(255,107,53,0.15)"
        homeProb={elo.home_prob}
        weight={elo.weight}
        narrative={elo.narrative}
        homeName={homeName}
        awayName={awayName}
      />

      <ComponentBar
        label="Recent Form"
        icon="🔥"
        iconBg="rgba(245,158,11,0.15)"
        homeProb={form.home_prob}
        weight={form.weight}
        narrative={form.narrative}
        homeName={homeName}
        awayName={awayName}
      />

      <ComponentBar
        label="Head-to-Head"
        icon="⚔️"
        iconBg="rgba(139,92,246,0.15)"
        homeProb={h2h.home_prob}
        weight={h2h.weight}
        narrative={h2h.narrative}
        homeName={homeName}
        awayName={awayName}
      />

      {/* Schedule factors */}
      <div className="component-row">
        <div className="component-header">
          <div className="component-label">
            <div className="component-icon" style={{ background: 'rgba(0,212,255,0.12)' }}>🗓️</div>
            Schedule & Rest
          </div>
          <span className="component-weight">
            Adj {schedule.adjustment >= 0 ? '+' : ''}{(schedule.adjustment * 100).toFixed(1)}pp
          </span>
        </div>

        <div className="schedule-box">
          {scheduleWarnings.slice(0, 4).map((item, i) => (
            <div key={i} className={`schedule-tag ${item.warning ? 'warning' : 'ok'}`}>
              <span>{item.warning ? '⚠️' : '✓'}</span>
              <span>{item.label}</span>
            </div>
          ))}
        </div>

        <div className="component-narrative">{schedule.narrative}</div>
      </div>

      {/* Final verdict */}
      <div style={{
        padding: '14px 16px',
        borderRadius: 'var(--radius-sm)',
        background: 'linear-gradient(135deg, rgba(255,107,53,0.08), rgba(0,212,255,0.04))',
        border: '1px solid rgba(255,107,53,0.2)',
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        gap: 12,
      }}>
        <div style={{ fontSize: '0.72rem', color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.08em' }}>
          Final Probability
        </div>
        <div style={{ display: 'flex', gap: 16, alignItems: 'center' }}>
          <div style={{ textAlign: 'center' }}>
            <div style={{ fontSize: '1.1rem', fontWeight: 800, color: 'var(--orange-bright)', letterSpacing: '-0.02em' }}>
              {(analysis.final_home_prob * 100).toFixed(1)}%
            </div>
            <div style={{ fontSize: '0.65rem', color: 'var(--text-muted)' }}>{homeName}</div>
          </div>
          {analysis.draw_prob != null && (
            <div style={{ textAlign: 'center' }}>
              <div style={{ fontSize: '1.1rem', fontWeight: 800, color: 'var(--yellow)', letterSpacing: '-0.02em' }}>
                {(analysis.draw_prob * 100).toFixed(1)}%
              </div>
              <div style={{ fontSize: '0.65rem', color: 'var(--text-muted)' }}>Draw</div>
            </div>
          )}
          <div style={{ textAlign: 'center' }}>
            <div style={{ fontSize: '1.1rem', fontWeight: 800, color: 'var(--cyan)', letterSpacing: '-0.02em' }}>
              {(analysis.final_away_prob * 100).toFixed(1)}%
            </div>
            <div style={{ fontSize: '0.65rem', color: 'var(--text-muted)' }}>{awayName}</div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default AlgorithmBreakdown;
