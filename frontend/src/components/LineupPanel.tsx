import React, { useState, useEffect } from 'react';
import { NbaPlayerStats, apiService } from '../services/api';

interface Props {
  homeTeamId: string;
  awayTeamId: string;
  homeTeamName: string;
  awayTeamName: string;
}

const POSITION_ORDER: Record<string, number> = {
  'G': 0, 'G-F': 1, 'F-G': 1, 'F': 2, 'F-C': 3, 'C-F': 3, 'C': 4, '': 5,
};

const positionSort = (a: NbaPlayerStats, b: NbaPlayerStats) => {
  const pa = POSITION_ORDER[a.position] ?? 5;
  const pb = POSITION_ORDER[b.position] ?? 5;
  if (pa !== pb) return pa - pb;
  return b.pts - a.pts;
};

const positionColor: Record<string, string> = {
  'G':   'rgba(0,212,255,0.15)',
  'G-F': 'rgba(0,212,255,0.1)',
  'F-G': 'rgba(139,92,246,0.1)',
  'F':   'rgba(139,92,246,0.15)',
  'F-C': 'rgba(255,107,53,0.1)',
  'C-F': 'rgba(255,107,53,0.1)',
  'C':   'rgba(255,107,53,0.15)',
};

const positionTextColor: Record<string, string> = {
  'G':   '#00D4FF',
  'G-F': '#00D4FF',
  'F-G': '#8B5CF6',
  'F':   '#8B5CF6',
  'F-C': '#FF8C42',
  'C-F': '#FF8C42',
  'C':   '#FF8C42',
};

const fmtPct = (n: number) => n > 0 ? `${(n * 100).toFixed(1)}%` : '–';
const fmtStat = (n: number) => n > 0 ? n.toFixed(1) : '–';

interface PlayerRowProps {
  player: NbaPlayerStats;
  isKey: boolean;
}

const PlayerRow: React.FC<PlayerRowProps> = ({ player, isKey }) => {
  const bgColor = positionColor[player.position] || 'transparent';
  const textColor = positionTextColor[player.position] || 'var(--text-muted)';

  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: '26px 1fr 34px 46px 46px 46px 42px 42px',
        gap: 6,
        alignItems: 'center',
        padding: '7px 10px',
        borderRadius: 8,
        background: isKey ? 'rgba(255,107,53,0.06)' : 'transparent',
        borderLeft: isKey ? '2px solid var(--orange)' : '2px solid transparent',
        transition: 'background 0.15s',
      }}
      onMouseEnter={e => (e.currentTarget.style.background = 'rgba(255,255,255,0.04)')}
      onMouseLeave={e => (e.currentTarget.style.background = isKey ? 'rgba(255,107,53,0.06)' : 'transparent')}
    >
      {/* Position badge */}
      <div
        style={{
          fontSize: '0.6rem',
          fontWeight: 700,
          padding: '2px 4px',
          borderRadius: 4,
          background: bgColor,
          color: textColor,
          textAlign: 'center',
          whiteSpace: 'nowrap',
          letterSpacing: '-0.03em',
        }}
      >
        {player.position || '–'}
      </div>

      {/* Name */}
      <div style={{ fontSize: '0.82rem', fontWeight: isKey ? 700 : 500, color: isKey ? 'var(--text-primary)' : 'var(--text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {player.first_name[0]}. {player.last_name}
        {player.jersey_number && (
          <span style={{ fontSize: '0.68rem', color: 'var(--text-muted)', marginLeft: 4 }}>
            #{player.jersey_number}
          </span>
        )}
      </div>

      {/* MIN */}
      <div style={{ fontSize: '0.72rem', color: 'var(--text-muted)', textAlign: 'center' }}>{player.min}</div>
      {/* PTS */}
      <div style={{ fontSize: '0.78rem', fontWeight: 700, color: player.pts >= 20 ? 'var(--orange-bright)' : player.pts >= 12 ? 'var(--text-primary)' : 'var(--text-secondary)', textAlign: 'center' }}>{fmtStat(player.pts)}</div>
      {/* REB */}
      <div style={{ fontSize: '0.72rem', color: 'var(--text-secondary)', textAlign: 'center' }}>{fmtStat(player.reb)}</div>
      {/* AST */}
      <div style={{ fontSize: '0.72rem', color: 'var(--text-secondary)', textAlign: 'center' }}>{fmtStat(player.ast)}</div>
      {/* FG% */}
      <div style={{ fontSize: '0.68rem', color: 'var(--text-muted)', textAlign: 'center' }}>{fmtPct(player.fg_pct)}</div>
      {/* 3P% */}
      <div style={{ fontSize: '0.68rem', color: 'var(--text-muted)', textAlign: 'center' }}>{fmtPct(player.fg3_pct)}</div>
    </div>
  );
};

const TableHeader: React.FC = () => (
  <div
    style={{
      display: 'grid',
      gridTemplateColumns: '26px 1fr 34px 46px 46px 46px 42px 42px',
      gap: 6,
      padding: '4px 10px 8px',
      fontSize: '0.62rem',
      fontWeight: 700,
      color: 'var(--text-muted)',
      textTransform: 'uppercase',
      letterSpacing: '0.08em',
    }}
  >
    <div>POS</div>
    <div>PLAYER</div>
    <div style={{ textAlign: 'center' }}>MIN</div>
    <div style={{ textAlign: 'center' }}>PTS</div>
    <div style={{ textAlign: 'center' }}>REB</div>
    <div style={{ textAlign: 'center' }}>AST</div>
    <div style={{ textAlign: 'center' }}>FG%</div>
    <div style={{ textAlign: 'center' }}>3P%</div>
  </div>
);

const TeamRoster: React.FC<{
  players: NbaPlayerStats[];
  teamName: string;
  accentColor: string;
  loading: boolean;
}> = ({ players, teamName, accentColor, loading }) => {
  const sorted = [...players].sort(positionSort).slice(0, 12);
  const keyPlayers = new Set(
    sorted
      .filter(p => p.pts >= 15 || p.pts === Math.max(...sorted.map(x => x.pts)))
      .map(p => p.player_id)
  );

  return (
    <div style={{ flex: 1, minWidth: 0 }}>
      {/* Team header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          marginBottom: 10,
          paddingBottom: 8,
          borderBottom: `1px solid ${accentColor}40`,
        }}
      >
        <div
          style={{
            width: 3,
            height: 18,
            borderRadius: 2,
            background: accentColor,
            boxShadow: `0 0 8px ${accentColor}80`,
          }}
        />
        <span style={{ fontSize: '0.8rem', fontWeight: 700, color: 'var(--text-primary)', letterSpacing: '-0.01em' }}>
          {teamName}
        </span>
      </div>

      {loading ? (
        <div style={{ padding: '16px 0', textAlign: 'center', color: 'var(--text-muted)', fontSize: '0.78rem' }}>
          Loading roster…
        </div>
      ) : sorted.length === 0 ? (
        <div style={{ padding: '16px 0', textAlign: 'center', color: 'var(--text-muted)', fontSize: '0.78rem' }}>
          No player data available yet
        </div>
      ) : (
        <>
          <TableHeader />
          {sorted.map(p => (
            <PlayerRow key={p.player_id} player={p} isKey={keyPlayers.has(p.player_id)} />
          ))}
        </>
      )}
    </div>
  );
};

const LineupPanel: React.FC<Props> = ({ homeTeamId, awayTeamId, homeTeamName, awayTeamName }) => {
  const [homePlayers, setHomePlayers] = useState<NbaPlayerStats[]>([]);
  const [awayPlayers, setAwayPlayers] = useState<NbaPlayerStats[]>([]);
  const [loadingHome, setLoadingHome] = useState(true);
  const [loadingAway, setLoadingAway] = useState(true);

  useEffect(() => {
    setLoadingHome(true);
    setLoadingAway(true);

    apiService.getTeamPlayers(homeTeamId).then(p => {
      setHomePlayers(p);
      setLoadingHome(false);
    }).catch(() => setLoadingHome(false));

    apiService.getTeamPlayers(awayTeamId).then(p => {
      setAwayPlayers(p);
      setLoadingAway(false);
    }).catch(() => setLoadingAway(false));
  }, [homeTeamId, awayTeamId]);

  return (
    <div
      style={{
        borderTop: '1px solid var(--border)',
        padding: '16px 20px',
        display: 'flex',
        flexDirection: 'column',
        gap: 16,
      }}
    >
      {/* Section title */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          fontSize: '0.68rem',
          fontWeight: 700,
          color: 'var(--text-muted)',
          textTransform: 'uppercase',
          letterSpacing: '0.12em',
        }}
      >
        <span>🏀 Likely Lineup · 2025-26 Season Averages</span>
        <div style={{ flex: 1, height: 1, background: 'var(--border)' }} />
      </div>

      {/* Two-column layout on wider screens */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1fr 1fr',
          gap: 20,
        }}
      >
        <TeamRoster
          players={homePlayers}
          teamName={homeTeamName}
          accentColor="var(--orange)"
          loading={loadingHome}
        />
        <TeamRoster
          players={awayPlayers}
          teamName={awayTeamName}
          accentColor="var(--cyan)"
          loading={loadingAway}
        />
      </div>

      {/* Legend */}
      <div
        style={{
          display: 'flex',
          gap: 12,
          flexWrap: 'wrap',
          fontSize: '0.65rem',
          color: 'var(--text-muted)',
          paddingTop: 8,
          borderTop: '1px solid var(--border)',
        }}
      >
        {[
          { pos: 'G', label: 'Guard' },
          { pos: 'F', label: 'Forward' },
          { pos: 'C', label: 'Center' },
        ].map(({ pos, label }) => (
          <div key={pos} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <div style={{ width: 8, height: 8, borderRadius: 2, background: positionColor[pos] || 'transparent', border: `1px solid ${positionTextColor[pos]}40` }} />
            <span>{pos} — {label}</span>
          </div>
        ))}
        <span style={{ marginLeft: 'auto' }}>Orange border = key player (15+ PPG)</span>
      </div>
    </div>
  );
};

export default LineupPanel;
