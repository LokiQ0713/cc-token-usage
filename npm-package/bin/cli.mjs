#!/usr/bin/env node

import { execSync, spawnSync } from "child_process";
import { existsSync, mkdirSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { tmpdir, homedir, platform } from "os";

const __dirname = dirname(fileURLToPath(import.meta.url));

// ─── Find binary ─────────────────────────────────────────────────────────────

function findBinary() {
  const isWin = platform() === "win32";
  const binName = isWin ? "cc-token-usage.exe" : "cc-token-usage";

  // 1. Check bundled binary
  const bundled = join(__dirname, binName);
  if (existsSync(bundled)) return bundled;

  // 2. Check PATH
  try {
    const which = isWin ? "where" : "which";
    const target = isWin ? "cc-token-usage.exe" : "cc-token-usage";
    const result = execSync(`${which} ${target}`, {
      encoding: "utf-8",
    }).trim();
    if (result) return result.split("\n")[0]; // `where` on Windows may return multiple
  } catch {}

  // 3. Check cargo bin
  const cargoBin = join(homedir(), ".cargo", "bin", binName);
  if (existsSync(cargoBin)) return cargoBin;

  console.error("Error: cc-token-usage binary not found.");
  console.error("Install it with: cargo install cc-token-usage");
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
    execSync(`${openCmd} "${htmlFile}"`, { stdio: "ignore" });
  } catch {
    // Silent fail — user can open manually
  }
}
