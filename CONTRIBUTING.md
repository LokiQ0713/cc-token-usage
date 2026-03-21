# Contributing to cc-token-usage

Thanks for your interest in contributing! Here's how to get started.

## Development Setup

```bash
git clone https://github.com/LokiQ0713/cc-token-usage.git
cd cc-token-usage
cargo build
cargo test
```

## Common Commands

```bash
# Run the tool
cargo run

# Run with arguments
cargo run -- --format html
cargo run -- project --top 5
cargo run -- session --latest
cargo run -- trend --days 30

# Run tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## Pull Requests

- Make sure `cargo test` passes
- Make sure `cargo clippy -- -D warnings` is clean
- Keep PRs focused — one feature or fix per PR
- Write descriptive commit messages
- Add tests for new functionality when possible

## Issues

### Bug Reports

Please include:
- What you expected to happen
- What actually happened
- Steps to reproduce
- Your OS and install method (npm or cargo)
- Output of `cc-token-usage --version`

### Feature Requests

Describe the use case and why it would be useful. If you have ideas about implementation, feel free to share them.

## Code Style

- Follow existing patterns in the codebase
- Treat clippy warnings as errors (`-D warnings`)
- Run `cargo fmt` before committing
- Keep functions focused and well-named
