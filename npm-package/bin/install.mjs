#!/usr/bin/env node

// Postinstall: resolve platform binary from optionalDependencies or PATH.

import { execSync } from "child_process";
import { existsSync, copyFileSync, chmodSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { createRequire } from "module";

const __dirname = dirname(fileURLToPath(import.meta.url));
const isWindows = process.platform === "win32";
const binaryName = isWindows ? "cc-token-usage.exe" : "cc-token-usage";
const binaryDest = join(__dirname, binaryName);

// Already have it
if (existsSync(binaryDest)) {
  process.exit(0);
}

// Try to resolve from platform-specific optionalDependency
const platform = process.platform;
const arch = process.arch;
const pkgMap = {
  "darwin-arm64": "cc-token-usage-darwin-arm64",
  "darwin-x64": "cc-token-usage-darwin-x64",
  "linux-x64": "cc-token-usage-linux-x64",
  "linux-arm64": "cc-token-usage-linux-arm64",
  "win32-x64": "cc-token-usage-windows-x64",
};
const pkgName = pkgMap[`${platform}-${arch}`];

if (pkgName) {
  try {
    const require = createRequire(import.meta.url);
    const pkgDir = dirname(require.resolve(`${pkgName}/package.json`));
    const src = join(pkgDir, binaryName);
    if (existsSync(src)) {
      copyFileSync(src, binaryDest);
      if (!isWindows) {
        chmodSync(binaryDest, 0o755);
      }
      console.log(`cc-token-usage: installed binary from ${pkgName}`);
      process.exit(0);
    }
  } catch {
    // Package not installed (wrong platform, or not published yet)
  }
}

// Fallback: check PATH
try {
  execSync("cc-token-usage --version", { stdio: "ignore" });
  process.exit(0);
} catch {
  console.warn("cc-token-usage: no pre-built binary for your platform.");
  console.warn("Install via cargo: cargo install cc-token-usage");
}
