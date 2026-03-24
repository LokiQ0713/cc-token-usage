#!/usr/bin/env node

import { execFileSync, spawnSync } from "child_process";
import { existsSync, mkdirSync, accessSync, chmodSync, constants } from "fs";
import { join, dirname } from "path";
import { createRequire } from "module";
import { tmpdir, platform, arch, homedir } from "os";

const require = createRequire(import.meta.url);

// ─── Resolve platform binary (Biome bin pattern) ────────────────────────────

const PLATFORMS = {
  "darwin-arm64": "cc-token-usage-darwin-arm64",
  "darwin-x64": "cc-token-usage-darwin-x64",
  "linux-x64": "cc-token-usage-linux-x64",
  "linux-arm64": "cc-token-usage-linux-arm64",
};

function findBinary() {
  const key = `${platform()}-${arch()}`;
  const pkg = PLATFORMS[key];

  // 1. Resolve from platform-specific optionalDependency
  if (pkg) {
    try {
      const pkgDir = dirname(require.resolve(`${pkg}/package.json`));
      const bin = join(pkgDir, "bin", "cc-token-usage");
      if (existsSync(bin)) {
        // Self-healing: ensure binary is executable
        try {
          accessSync(bin, constants.X_OK);
        } catch {
          try { chmodSync(bin, 0o755); } catch {}
        }
        return bin;
      }
    } catch {}
  }

  // 2. Fallback: check PATH
  try {
    const which = platform() === "win32" ? "where" : "which";
    const result = execFileSync(which, ["cc-token-usage"], {
      encoding: "utf-8",
    }).trim();
    if (result) return result;
  } catch {}

  // 3. Fallback: check cargo bin
  const cargoBin = join(homedir(), ".cargo", "bin", "cc-token-usage");
  if (existsSync(cargoBin)) return cargoBin;

  console.error("Error: cc-token-usage binary not found.");
  console.error(`Unsupported platform: ${platform()}-${arch()}`);
  console.error("Install via cargo: cargo install cc-token-usage");
  process.exit(1);
}

// ─── Parse args ──────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const hasFormatFlag = args.some((a) => a === "--format");
const hasOutputFlag = args.some((a) => a === "--output");
const wantsHelp = args.includes("--help") || args.includes("-h");
const wantsVersion = args.includes("--version") || args.includes("-V");

// ─── Simple passthrough for help/version/explicit args ───────────────────────

if (wantsHelp || wantsVersion || hasFormatFlag) {
  const binary = findBinary();
  const result = spawnSync(binary, args, { stdio: "inherit" });
  process.exit(result.status ?? 1);
}

// ─── Default behavior: summary + HTML ────────────────────────────────────────

const binary = findBinary();
const subcommand = args[0] || "overview";
const restArgs = args.slice(1);

// 1. Print terminal summary
const textResult = spawnSync(binary, [subcommand, ...restArgs], {
  stdio: "inherit",
});

if (textResult.status !== 0) {
  process.exit(textResult.status ?? 1);
}

// 2. Generate HTML report
const outputDir = join(tmpdir(), "cc-token-usage");
mkdirSync(outputDir, { recursive: true });

const htmlFile =
  subcommand === "session"
    ? join(outputDir, "session-report.html")
    : join(outputDir, "report.html");

const htmlArgs = [subcommand, "--format", "html", "--output", htmlFile, ...restArgs];
const htmlResult = spawnSync(binary, htmlArgs, { stdio: "pipe" });

if (htmlResult.status === 0) {
  console.log(`\nHTML report: ${htmlFile}`);

  // 3. Auto-open in browser
  const openCmd =
    process.platform === "darwin"
      ? "open"
      : process.platform === "win32"
        ? "start"
        : "xdg-open";

  try {
    execFileSync(openCmd, [htmlFile], { stdio: "ignore" });
  } catch {
    // Silent fail — user can open manually
  }
}
