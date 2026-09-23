const { spawn } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const root = path.join(__dirname, "..");
const cargoBin = path.join(os.homedir(), ".cargo", "bin");

function cargoTargetDir() {
  // GitHub's tauri-action looks under src-tauri/target; keep Cargo's default on CI.
  if (process.env.CI || process.env.GITHUB_ACTIONS) {
    return null;
  }
  const home = os.homedir();
  switch (process.platform) {
    case "win32":
      return path.join(
        process.env.LOCALAPPDATA || path.join(home, "AppData", "Local"),
        "StatTracker",
        "target"
      );
    case "darwin":
      return path.join(home, "Library", "Application Support", "StatTracker", "target");
    default:
      return path.join(
        process.env.XDG_DATA_HOME || path.join(home, ".local", "share"),
        "StatTracker",
        "target"
      );
  }
}

const targetDir = cargoTargetDir();

process.env.PATH = `${cargoBin}${path.delimiter}${process.env.PATH}`;
if (targetDir) {
  process.env.CARGO_TARGET_DIR = targetDir;
  fs.mkdirSync(targetDir, { recursive: true });
}

const tauriCli = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const args = process.argv.slice(2);
const child = spawn(process.execPath, [tauriCli, ...args], {
  stdio: "inherit",
  env: process.env,
  cwd: root,
});

child.on("exit", (code, signal) => {
  if (signal) {
    // Re-raise so parents see the same signal; fall back to the shell
    // convention (128 + signal number) when that is not possible.
    try {
      process.kill(process.pid, signal);
    } catch {
      // Unknown or unraisable on this OS (typical on Windows).
    }
    const signalNumber = os.constants.signals[signal];
    process.exit(typeof signalNumber === "number" ? 128 + signalNumber : 1);
  }
  process.exit(code ?? 1);
});
