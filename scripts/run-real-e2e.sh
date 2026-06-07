#!/usr/bin/env bash
# Run all #[ignore]'d real-data e2e tests against the developer's local
# ~/.claude history. Used as pre-release validation — tests that rely on
# specific reference sessions (e.g. ae289b37) panic loudly under
# REQUIRE_REAL_DATA=1 instead of silently passing.
#
# Usage:
#   scripts/run-real-e2e.sh
#
# Exit code is non-zero if any ignored test fails.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [[ ! -d "${HOME}/.claude" ]]; then
    echo "ERROR: ~/.claude does not exist on this machine." >&2
    echo "These tests need real Claude Code session history to validate against." >&2
    exit 2
fi

echo "==> Running ignored real-data e2e tests with REQUIRE_REAL_DATA=1"
echo "    (a missing reference session will now panic, not silently skip)"

REQUIRE_REAL_DATA=1 cargo test --workspace --all-features -- --ignored "$@"

echo
echo "==> All real-data e2e tests passed."
