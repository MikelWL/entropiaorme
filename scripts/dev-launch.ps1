$ErrorActionPreference = "Stop"

# Resolve the repo root from this script's location (scripts/ subdir).
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$frontendDir = Join-Path $repoRoot "frontend"

# The app is a single binary: the backend runs in-process inside the Tauri
# shell in every mode, including `tauri dev`, and the webview reaches it over
# Tauri IPC (no localhost backend port, no proxy, no sidecar). So dev launch is
# just the one Tauri dev process, which spawns and tails Vite for the frontend.
#
# Env vars (ENTROPIAORME_FRONTEND_PORT / ENTROPIAORME_DATA_DIR) are sourced
# from .env.local by `just` itself via `set dotenv-load` in the justfile, and
# inherited here. FRONTEND_PORT drives Vite's port and the dev URL; DATA_DIR
# points the in-process backend at a per-checkout data directory. Parallel
# checkouts coexist by giving each its own values in its own .env.local.

if (-not (Test-Path $frontendDir)) {
    Write-Error "Missing frontend directory at $frontendDir."
}

# Launch the single dev process in the foreground. `tauri:dev` runs
# build-dev-config.mjs (which writes the env-driven devUrl overlay) and then
# `tauri dev`, which builds and runs the shell with the backend in-process and
# spawns Vite as a child, tailing both. Ctrl+C tears the whole stack down.
npm --prefix $frontendDir run tauri:dev
