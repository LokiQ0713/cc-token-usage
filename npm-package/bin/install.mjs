#!/usr/bin/env node

// Postinstall script: download the pre-compiled Rust binary for the current platform.
// For now, checks if cc-token-usage is already available via cargo install.

import { execSync } from "child_process";
import { existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const binaryPath = join(__dirname, "cc-token-usage");

// Platform-specific binary names
const platform = process.platform;
const arch = process.arch;
const binaryName =
  platform === "win32" ? "cc-token-usage.exe" : "cc-token-usage";

// Check if binary already bundled
if (existsSync(join(__dirname, binaryName))) {
  process.exit(0);
}

// Check if available in PATH (installed via cargo)
try {
  execSync("cc-token-usage --version", { stdio: "ignore" });
  console.log("cc-token-usage found in PATH (installed via cargo)");
  process.exit(0);
} catch {
  // Not found — try cargo install
}

// Try to build from source if cargo is available
try {
  execSync("cargo --version", { stdio: "ignore" });
  console.log("Building cc-token-usage from source...");
  console.log("This may take a minute on first install.");

  const crateDir = join(__dirname, "..", "..", "cc-token-usage");
  if (existsSync(join(crateDir, "Cargo.toml"))) {
    execSync(`cargo install --path "${crateDir}"`, { stdio: "inherit" });
    console.log("cc-token-usage installed successfully via cargo.");
  } else {
    console.warn(
      "Warning: cc-token-usage binary not found. Install it with:"
    );
    console.warn(
      "  cargo install --path /path/to/cc-token-usage"
    );
  }
} catch {
  console.warn("Warning: Could not install cc-token-usage.");
  console.warn("Install Rust (https://rustup.rs) then run:");
  console.warn(
    "  cargo install --path /path/to/cc-token-usage"
  );
}
