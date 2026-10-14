import { spawnSync } from "node:child_process";

const args = ["test", "--manifest-path", "src-tauri/Cargo.toml", "--locked"];
if (process.platform === "win32") {
  args.push("--config", "src-tauri/.cargo/test-runner.toml");
}

const result = spawnSync("cargo", args, { stdio: "inherit" });
if (result.error) {
  console.error(result.error);
}
process.exit(result.status ?? 1);
