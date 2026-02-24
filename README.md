# OddsForge 🎯

A full-stack sports analytics platform built with **Rust** (backend) and **React + TypeScript** (frontend). It uses an **ELO rating engine** and an **ensemble prediction model** to forecast match outcomes for the EPL and NBA, then surfaces *market edges* where the model disagrees with betting-market implied probabilities.

No external API keys required — the app seeds itself with realistic data on first launch.

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust · Axum · SQLx · SQLite |
| Predictions | ELO system · Ensemble model (ELO + H2H + form) |
| Frontend | React 19 · TypeScript · react-router-dom |
| Charts | Recharts |
| Icons | Lucide React |

---

## Architecture

```
┌─────────────────────┐        ┌──────────────────────┐        ┌──────────────┐
│  React Frontend     │──HTTP──▶  Rust API (Axum)      │──SQLx──▶  SQLite DB   │
│  localhost:3001     │        │  localhost:3000        │        │  data/       │
└─────────────────────┘        └──────────────────────┘        └──────────────┘
```

---

## Features

### Dashboard
- Live-updating cards for upcoming EPL & NBA matches
- Animated win-probability bars (home / draw / away)
- League filter tabs and confidence meters
- Stats summary: total matches, predictions count, high-confidence picks

### Edge Finder
- Sortable table of matches where our model's probability differs from market implied odds by **>5%**
- Columns: Match · League · Our Prediction · Market Implied · Market Odds · Edge % · Confidence
- Colour-coded edge badges (green / amber / grey by magnitude)

### Dataset Builder
- Form-driven interface: sport, date range, data categories (basic / teams / predictions)
- Exports CSV or JSON from the database via a REST call
- Preview panel shows which columns will be included

### Team Profiles
- Searchable sidebar listing all 50 teams (20 EPL + 30 NBA)
- ELO rating history line chart (Recharts)
- Season stats: W / D / L, goals, points-per-game, win rate
- Recent results table with W / D / L badges

---

## Project Structure

```
OddsForge/
├── backend/                     # Rust API server
│   ├── src/
│   │   ├── api/mod.rs           # REST endpoints + CORS
│   │   ├── db/
│   │   │   ├── mod.rs           # Query helpers
│   │   │   └── seed.rs          # Seed data (20 EPL + 30 NBA teams, 50 matches)
│   │   ├── models/mod.rs        # Shared data types
│   │   ├── services/
│   │   │   ├── elo_calculator.rs
│   │   │   ├── predictor.rs     # Ensemble model
│   │   │   └── data_fetcher.rs  # Optional external API client
│   │   ├── cli/mod.rs           # CLI subcommands
│   │   └── main.rs
│   └── Cargo.toml
├── frontend/                    # React app
│   ├── src/
│   │   ├── pages/
│   │   │   ├── Dashboard.tsx
│   │   │   ├── EdgeFinder.tsx
│   │   │   ├── DatasetBuilder.tsx
│   │   │   └── TeamProfile.tsx
│   │   ├── services/api.ts      # Axios API client
│   │   ├── App.tsx
│   │   └── index.css            # Dark theme (CSS variables)
│   └── package.json
├── data/                        # Auto-created at runtime
│   └── exports/
└── README.md
```

---

## Quick Start

### Prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Node.js ≥ 18
brew install node@22          # macOS
```

### 1 — Start the backend

```bash
cd backend
cargo run                    # defaults to: serve --port 3000
# The database is created and seeded automatically on first run.
```

### 2 — Start the frontend (new terminal)

```bash
cd frontend
npm install
npm start                    # http://localhost:3001
```

That's it. No API keys, no external databases, no environment variables required.

---

## API Endpoints

```
GET  /health                        Health check
GET  /matches/upcoming?sport=&limit= Upcoming matches with predictions
GET  /teams                         All teams
GET  /teams/league/:sport/:league    Teams filtered by league
GET  /teams/:id/stats               Team profile (stats, ELO history, recent matches)
GET  /predictions/edges             Market edge opportunities
POST /datasets/generate             Export dataset (CSV or JSON)
POST /data/fetch                    Trigger external API sync (optional, needs API key)
POST /predictions/generate          Re-run prediction engine
```

Example:
```bash
curl http://localhost:3000/matches/upcoming?sport=football
curl http://localhost:3000/predictions/edges
curl http://localhost:3000/teams/epl_1/stats
```

---

## Prediction Model

### ELO Rating System
- Starting ratings: EPL teams ~1200–1510; NBA teams ~1170–1540
- Home advantage: +100 ELO points
- Goal/point-difference multiplier on updates
- Season progression tracked in `elo_history` table

### Ensemble Model (three components)
| Model | Weight | Description |
|-------|--------|-------------|
| ELO-based | 50% | Pure ELO rating differential |
| Head-to-head | 30% | Historical matchup record with mean-regression |
| Form-based | 20% | Sigmoid of ELO diff with home bonus |

### Football draw handling
`draw_probability = 0.25` (base), then home/away scaled proportionally and normalised to sum to 1.

### Market edges
`edge = our_probability − (1 / market_odds)`; only edges > 5% surface in the Edge Finder.

---

## CLI Commands

```bash
cargo run -- serve --port 3000    # Start API server (default)
cargo run -- init-db              # Create schema only
cargo run -- fetch --sport all    # Fetch from external APIs (needs API key)
cargo run -- predict              # Regenerate predictions
cargo run -- team --name Arsenal  # Query team from terminal
```

---

## Environment Variables

```bash
# backend/.env (optional — defaults work without it)
DATABASE_URL=sqlite:../data/oddsforge.db
FOOTBALL_DATA_API_KEY=your_key   # Only needed for live EPL data
RUST_LOG=info
```

---

## Seed Data (no API key needed)

| Category | Count |
|----------|-------|
| EPL teams | 20 (all 2025-26 clubs) |
| NBA teams | 30 (full league) |
| Historical matches | 30 (EPL + NBA, with realistic scores) |
| Upcoming matches | 30 (next 6 weeks, with predictions) |
| ELO history points | 84 (top teams, 6-month progression) |
| Season stats | 50 teams |

---

*Built with Rust + React — ELO-powered sports prediction platform*
