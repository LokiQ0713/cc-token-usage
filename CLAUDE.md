# cc-token-usage

Analyze Claude Code session token usage, costs, and efficiency. Reads local JSONL session files, calculates token consumption and API-equivalent costs, and generates terminal summaries and interactive HTML dashboards.

- **GitHub:** https://github.com/LokiQ0713/cc-token-usage
- **npm:** https://www.npmjs.com/package/cc-token-usage

## Tech Stack

- **Language:** Rust (edition 2021)
- **Dependencies:** serde/serde_json (serialization), clap (CLI), chrono (dates), comfy-table (terminal tables), toml (config), dirs (paths), anyhow (errors)
- **Frontend:** Chart.js (embedded in HTML output)
- **Distribution:** cargo (crates.io) + npm (pre-built binaries)

## File Structure

```
src/
├── main.rs              # Entry point, CLI dispatch
├── cli.rs               # Clap CLI argument definitions
├── config.rs            # Config file loading (pricing overrides)
├── lib.rs               # Library root
├── data/
│   ├── mod.rs
│   ├── scanner.rs       # Discover JSONL session files
│   ├── parser.rs        # 5-stage pipeline: parse → filter → validate → extract → dedup
│   ├── models.rs        # Data types (JournalEntry, ValidatedTurn, SessionData, TokenUsage)
│   └── loader.rs        # Load sessions + merge agent turns (cross-file dedup)
├── pricing/
│   ├── mod.rs
│   └── calculator.rs    # Token cost calculation with cache tiers
├── analysis/
│   ├── mod.rs
│   ├── overview.rs      # Aggregate statistics
│   ├── project.rs       # Per-project breakdown
│   ├── session.rs       # Per-session breakdown
│   ├── trend.rs         # Daily/monthly trend analysis
│   └── validate.rs      # Independent token verification (dual-path cross-validation)
└── output/
    ├── mod.rs
    ├── text.rs           # Terminal table output
    └── html.rs           # Interactive HTML dashboard

npm-package/             # npm wrapper for binary distribution
config.example.toml      # Example config with pricing overrides
```

## Development

```bash
# Build
cargo build

# Run
cargo run
cargo run -- --format html
cargo run -- project --top 5
cargo run -- session --latest
cargo run -- trend --days 30
cargo run -- validate              # Verify token accuracy (3890+ checks)
cargo run -- validate --failures-only

# Test
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## Release Workflow

1. Bump version in `Cargo.toml` and `npm-package/package.json`
2. `npm version patch/minor/major` (creates git tag)
3. `git push && git push --tags`
4. GitHub Actions `release.yml` auto-builds binaries for all platforms and publishes to npm

## CI/CD

- **CI** (`ci.yml`): Runs `cargo check`, `cargo test`, `cargo clippy` on push/PR to master
- **Release** (`release.yml`): Triggered by version tags, builds cross-platform binaries, publishes to npm
