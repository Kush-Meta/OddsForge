# OddsForge 🎯

A Rust-powered sports analytics platform for prediction markets. Features ELO ratings, machine learning predictions, and market edge detection for EPL, Champions League, and NBA.

![OddsForge Architecture](docs/architecture.png)

## 🏗️ Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   React Frontend │────▶│   Rust Backend  │────▶│   SQLite DB     │
│   (Dashboard)    │     │   (API + ML)    │     │   (Sports Data) │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                │
                                ▼
                        ┌─────────────────┐
                        │   External APIs │
                        │ • football-data │
                        │ • balldontlie   │
                        └─────────────────┘
```

### Components

- **Backend (Rust)**: High-performance API server with prediction engine
- **Frontend (React + TypeScript)**: Modern dashboard with dark theme
- **Database (SQLite)**: Efficient local storage for teams, matches, and predictions
- **Data Sources**: Free sports APIs (football-data.org, balldontlie.io)

## ✨ Features

### 🏈 Sports Analytics
- **ELO Rating System**: Dynamic team strength calculations
- **Machine Learning**: Ensemble predictions combining multiple models
- **Head-to-Head Analysis**: Historical matchup insights
- **Form Tracking**: Recent performance impact on predictions

### 📊 Market Intelligence
- **Edge Detection**: Compare model predictions vs market odds
- **Confidence Scoring**: Model agreement analysis
- **Dataset Builder**: Custom CSV/JSON export for research

### 🎮 Multi-Sport Support
- ⚽ **Football**: EPL, Champions League (with draws)
- 🏀 **Basketball**: NBA (binary outcomes)

## 🚀 Quick Start

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node.js (for frontend)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 18
```

### Setup

1. **Clone and setup**:
```bash
git clone https://github.com/Kush-Meta/OddsForge.git
cd OddsForge
```

2. **Backend setup**:
```bash
cd backend
cp .env.example .env
# Optional: Add your football-data.org API key to .env
```

3. **Initialize database**:
```bash
cargo run -- init-db
```

4. **Fetch sports data** (free APIs):
```bash
cargo run -- fetch --sport all
```

5. **Generate predictions**:
```bash
cargo run -- predict
```

6. **Start API server**:
```bash
cargo run -- serve --port 3000
```

7. **Frontend setup** (in another terminal):
```bash
cd ../frontend
npm install
npm start
```

Visit [http://localhost:3001](http://localhost:3001) 🎉

## 🎯 CLI Usage

```bash
# Initialize database
cargo run -- init-db

# Fetch data for specific sport
cargo run -- fetch --sport football
cargo run -- fetch --sport basketball

# Generate predictions
cargo run -- predict

# Query team information
cargo run -- team --name "Arsenal"

# Start API server
cargo run -- serve --port 3000
```

## 🔌 API Endpoints

### Core Endpoints

```http
GET /health                    # Health check
GET /matches/upcoming          # Upcoming matches with predictions
GET /teams/{id}/stats         # Team analytics and profile
GET /predictions/edges        # Market edge opportunities
POST /datasets/generate       # Custom dataset builder
POST /data/fetch             # Fetch latest sports data
POST /predictions/generate   # Generate predictions
```

### Example Requests

```bash
# Get upcoming matches
curl http://localhost:3000/matches/upcoming?sport=football&limit=10

# Get team stats
curl http://localhost:3000/teams/epl_57/stats

# Find market edges
curl http://localhost:3000/predictions/edges

# Generate custom dataset
curl -X POST http://localhost:3000/datasets/generate \
  -H "Content-Type: application/json" \
  -d '{
    "sport": "football",
    "stats_categories": ["basic", "teams", "predictions"],
    "format": "csv"
  }'
```

## 🧠 Prediction Models

### ELO Rating System
- Dynamic team strength based on match results
- Home advantage factoring (+100 ELO points)
- Goal difference multipliers for accurate updates

### Ensemble Model
Combines multiple prediction approaches:

1. **ELO-based** (50% weight): Pure rating differential
2. **Head-to-head** (30% weight): Historical matchup analysis  
3. **Form-based** (20% weight): Recent performance trends

### Confidence Scoring
- Model agreement analysis
- Standard deviation across predictions
- Ranges from 50% (low confidence) to 100% (high confidence)

## 📁 Project Structure

```
OddsForge/
├── backend/                 # Rust API server
│   ├── src/
│   │   ├── api/            # REST API endpoints
│   │   ├── db/             # Database operations
│   │   ├── models/         # Data structures
│   │   ├── services/       # Business logic
│   │   │   ├── data_fetcher.rs    # Sports API integration
│   │   │   ├── elo_calculator.rs  # Rating system
│   │   │   └── predictor.rs       # ML models
│   │   ├── cli/            # Command line interface
│   │   └── utils/          # Helper functions
│   └── Cargo.toml
├── frontend/               # React dashboard
│   ├── src/
│   │   ├── components/     # UI components
│   │   ├── pages/          # Main pages
│   │   └── services/       # API client
│   └── package.json
├── data/                   # SQLite database & exports
│   ├── oddsforge.db
│   └── exports/
└── README.md
```

## 🌐 Data Sources

### Football Data (Optional - Requires API Key)
- **Source**: [football-data.org](https://www.football-data.org/)
- **Coverage**: EPL, Champions League
- **Rate Limit**: 10 requests/minute (free tier)
- **Features**: Team info, fixtures, results

### NBA Data (Free)
- **Source**: [balldontlie.io](https://www.balldontlie.io/)
- **Coverage**: NBA teams and games
- **Rate Limit**: None
- **Features**: Team stats, game results

## 🎨 Frontend Features

### Dashboard
- Upcoming matches with win probabilities
- League standings with ELO ratings
- Recent predictions accuracy

### Edge Finder
- Model vs market odds comparison
- Profitable betting opportunities
- Confidence-weighted recommendations

### Dataset Builder
- Custom data exports
- Multiple format support (CSV, JSON)
- Flexible filtering options

### Team Profiles
- Historical ELO rating charts
- Recent match results and form
- Head-to-head records

## ⚙️ Configuration

### Environment Variables

```bash
# Database
DATABASE_URL=sqlite:../data/oddsforge.db

# APIs (optional)
FOOTBALL_DATA_API_KEY=your_key_here

# Server
PORT=3000
RUST_LOG=info

# Model Parameters
ELO_K_FACTOR=32
HOME_ADVANTAGE=100
```

## 🧪 Development

### Run Tests
```bash
cargo test
```

### Code Formatting
```bash
cargo fmt
```

### Linting
```bash
cargo clippy
```

### Watch Mode
```bash
cargo watch -x run
```

## 📈 Roadmap

- [ ] **Advanced Models**: Neural networks, XGBoost
- [ ] **More Sports**: NFL, Premier League, La Liga
- [ ] **Live Data**: WebSocket real-time updates
- [ ] **Betting Integration**: Odds API integration
- [ ] **Mobile App**: React Native companion
- [ ] **Cloud Deploy**: Docker + AWS/Railway

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📜 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- [football-data.org](https://www.football-data.org/) for football data API
- [balldontlie.io](https://www.balldontlie.io/) for free NBA data
- Rust community for excellent crates
- Sports analytics community for inspiration

---

**Made with ❤️ and ⚡ Rust**

*Predict smarter, bet better* 🎯