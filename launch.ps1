$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path $PSScriptRoot).Path
$frontendDir = Join-Path $repoRoot "frontend"
$pythonExe = Join-Path $repoRoot ".venv\Scripts\python.exe"

if (-not (Get-Command wt.exe -ErrorAction SilentlyContinue)) {
    Write-Error "Windows Terminal (wt.exe) is required for launch.ps1."
}

if (-not (Test-Path $pythonExe)) {
    Write-Error "Missing backend interpreter at $pythonExe. Create the Windows virtualenv first."
}

if (-not (Test-Path $frontendDir)) {
    Write-Error "Missing frontend directory at $frontendDir."
}

$backendCmd = "`"$pythonExe`" -X utf8 -m backend.main"
$tauriCmd = "npm exec tauri dev"

Start-Process wt.exe -ArgumentList @(
    "-w", "0",
    "new-tab",
    "--title", "Backend",
    "-d", $repoRoot,
    "cmd", "/k", $backendCmd
)

Start-Sleep -Milliseconds 700

Start-Process wt.exe -ArgumentList @(
    "-w", "0",
    "new-tab",
    "--title", "Tauri",
    "-d", $frontendDir,
    "cmd", "/k", $tauriCmd
)
