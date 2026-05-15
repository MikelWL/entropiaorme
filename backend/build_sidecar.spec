# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller spec for the EntropiaOrme backend sidecar.

Produces a single-file Windows exe that boots `backend.main:app` on
127.0.0.1:8421 — the same listener the dev `uvicorn backend.main:app`
shell exposes, with frozen-aware path handling for game-data resources
and user-data writes (see backend/main.py).

Build invocation (from the repo root):

    .venv/Scripts/python.exe -m PyInstaller backend/build_sidecar.spec --noconfirm
"""

import os
from pathlib import Path

from PyInstaller.utils.hooks import collect_all, collect_submodules

# Paths — SPEC is the absolute path of this .spec file (PyInstaller-injected).
SPEC_DIR = Path(SPEC).parent  # backend/
PROJECT_ROOT = SPEC_DIR.parent

# Bundled read-only data: game-data snapshot, panel geometry, TT curve CSV,
# and the local OCR model weights (kept inside the frozen exe so OCR works
# fully offline from cold start; see `backend/services/local_ocr.py` for
# the load path resolution).
datas = []
snapshot_dir = SPEC_DIR / "data" / "snapshot"
for path in sorted(snapshot_dir.glob("*.json")):
    datas.append((str(path), "backend/data/snapshot"))
datas.append((str(SPEC_DIR / "data" / "panel_geometry.json"), "backend/data"))
datas.append((str(SPEC_DIR / "data" / "tt_value_curve.csv"), "backend/data"))
datas.append((str(SPEC_DIR / "assets" / "models" / "svtrv2_rec.onnx"), "backend/assets/models"))

# Bundled curated demo DB consumed by the /demo/* router under guide-mode
# playback. The demo router resolves `<bundle_root>/data/demo/entropia_orme.db`
# in frozen mode via `sys._MEIPASS` (see backend/routers/demo.py).
datas.append((str(PROJECT_ROOT / "data" / "demo" / "entropia_orme.db"), "data/demo"))
datas.append((str(PROJECT_ROOT / "data" / "demo" / "seed_manifest.json"), "data/demo"))

binaries = []
hiddenimports = []

# Routers register endpoints via FastAPI side-effects on import; the
# `from backend.routers import health, ...` block in main.py covers
# them statically, but list them explicitly for resilience against
# refactors.
hiddenimports += [
    "backend.routers.health",
    "backend.routers.character",
    "backend.routers.equipment",
    "backend.routers.settings",
    "backend.routers.tracking",
    "backend.routers.analytics",
    "backend.routers.codex",
    "backend.routers.quests",
    "backend.routers.scan_manual",
    "backend.routers.demo",
    # demo router lazy-imports these at first /demo/tracking/* hit; declare
    # explicitly so PyInstaller bundles them despite the deferred import.
    "backend.scripts.demo_seed.live_injection",
    "backend.scripts.demo_seed.canonical",
    "backend.scripts.demo_seed.contract",
]

# uvicorn dispatches to loop/protocol/lifespan implementations via
# string dotted-paths; PyInstaller can't follow those statically.
hiddenimports += collect_submodules("uvicorn")

# pynput's backend (win32 / xorg / darwin) is selected at import time
# via platform sniffing and runtime imports.
hiddenimports += collect_submodules("pynput")

# onnxruntime ships native providers as separate binaries; collect_all
# is the canonical way to handle it.
ort_datas, ort_binaries, ort_hiddenimports = collect_all("onnxruntime")
datas += ort_datas
binaries += ort_binaries
hiddenimports += ort_hiddenimports

# openocr-python loads YAML configs at runtime; collect the package's
# static resources so the wrapper has everything it needs. Model weights
# are bundled separately via the `assets/models/svtrv2_rec.onnx` datas
# entry above (the package's lazy ModelScope/HuggingFace download is
# bypassed: `local_ocr.OpenOcrEngine` passes `onnx_model_path=` directly).
ocr_datas, ocr_binaries, ocr_hiddenimports = collect_all("openocr")
datas += ocr_datas
binaries += ocr_binaries
hiddenimports += ocr_hiddenimports

# Dev tooling and OCR-adjacent submodules excluded so they stay out of
# the bundle. `onnxruntime.quantization` and `openocr.tools.data` reach
# for `torch` / `onnx` at import time; explicit exclusion silences the
# benign `collect_submodules` warnings the build surfaced and guards
# against a transitive openocr update accidentally pulling torch / onnx
# into the bundled tree.
#
# Note: `backend.scripts.demo_seed.live_injection` + `.canonical` are
# bundled (hiddenimports above) because the /demo/* router calls
# `prime_tracker` at first tracker-state hit. The CLI driver modules
# (`__main__`, `driver`, `domain_*`) are intentionally NOT in
# hiddenimports: PyInstaller's import walker won't pull them in since
# no production code references them, so the seeder remains a dev-only
# CLI without bloating the frozen bundle.
excludes = [
    "pytest",
    "mypy",
    "onnxruntime.quantization",
    "openocr.tools.data",
    "torch",
    "onnx",
]


a = Analysis(
    ["main.py"],
    pathex=[str(PROJECT_ROOT)],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=excludes,
    noarchive=False,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="entropiaorme-backend",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    runtime_tmpdir=None,
    # GUI-subsystem build: the sidecar is never user-facing; the Tauri
    # parent drains stdout/stderr via tauri-plugin-shell pipes regardless
    # of subsystem, so logs still surface in the shell's [backend] prefix.
    # Console-subsystem would risk a conhost flash on any spawn path that
    # doesn't suppress windows (the plugin-shell path does, but the spec
    # default shouldn't depend on every caller knowing that).
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
