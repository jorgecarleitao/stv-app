# STV Election App

This project provides a complete platform for running, managing, and explaining Single Transferable Vote (STV) elections.
It supports the full election lifecycle: setup, secure ballot distribution, voting, tallying, and transparent result explanation.

* [stv-app-infrastructure](https://github.com/jorgecarleitao/stv-app-infrastructure)
* [stv-app](https://github.com/jorgecarleitao/stv-app)
* [stv-app-frontend](https://github.com/jorgecarleitao/stv-app-frontend)

## Features

- **Full Election Workflow:**
  - Create and configure elections (candidates, seats, descriptions, start/end times)
  - Generate and manage unique ballot tokens for secure, auditable voting
  - Distribute tokens to voters; tokens can only be redeemed while the election is open
  - Voters use tokens to access their ballot and submit ranked preferences
  - Ballots are anonymized upon submission (token-voter link is erased)
  - Results are computed and published after the election closes

- **Advanced Tallying and Explanation:**
  - Uses Meek's method for STV with Droop quota ([reference](https://prfound.org/2020/05/droop-python-3/)), matching real-world proportional elections
  - Elected candidates are further ordered using [Copeland's method](https://en.wikipedia.org/wiki/Copeland%27s_method) for ranking them
  - Exposes both the Copeland order and the full pairwise comparison matrix for transparency
  - Frontend displays detailed results, including matrix, order, and individual ballots, for auditability

- **Modern Web UI:**
  - Election setup, admin, and voting flows
  - Ballot group management, foldable UI, and localized demo data (Portuguese/English)
  - Shareable election configuration via URL or YAML

- **Security and Integrity:**
  - Ballot tokens cannot be redeemed after the election closes
  - All critical actions (token creation, ballot submission) are auditable
  - Data can be persisted via Docker volume

## How It Works

The backend is written in Rust, using Axum and SeaORM, and implements:

- Election and ballot token management
- Secure, auditable voting (tokens, anonymization)
- STV tallying (Meek's method, Droop quota)
- Copeland pairwise ordering and matrix computation
- REST API for frontend and automation

The frontend (see `stv-app-frontend/`) is a modern React app for election setup, admin, and voting.

### Technical Details

- [STV algorithm implementation](https://github.com/gendx/stv-rs) by Guillaume Endignoux ([explanation](https://gendignoux.com/blog/2023/03/27/single-transferable-vote.html))
- Results include both the set of elected candidates and their order, plus a full pairwise matrix for transparency

## Quick Start

```bash
# Quick start (data will be lost when container stops)
docker run --rm -p 8080:8080 -e RUST_LOG=info ghcr.io/jorgecarleitao/stv-app:main

# Persistent data with volume mount
mkdir -p ./election-data
docker run --rm -p 8080:8080 \
  --user $(id -u):$(id -g) \
  -e RUST_LOG=info \
  -v ./election-data:/app/data \
  ghcr.io/jorgecarleitao/stv-app:main

# open http://localhost:8080 (or proxy TLS-terminated requests to it)
```

### Environment Variables

- `DATABASE_URL` — SQLite connection string (default in container: `sqlite:///app/data/elections.db`)
- `RUST_LOG` — Log level (default: `info`)
- `FRONTEND_STATIC_DIR` — Path to frontend static files (default in container: `/app/static`)

## Development

```bash
cargo test
RUST_LOG=info DATABASE_URL="sqlite:election-data/elections.db?mode=rwc" cargo run --bin cli
docker build -t test .
```
