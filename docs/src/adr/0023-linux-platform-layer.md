# ADR-0023: The Linux platform layer, on XWayland with kernel-level input and portal capture

- Status: Accepted
- Context: the application was authored Windows-first, with its OS-touching seams (global key observation, screen capture, game-window discovery, process-resource sampling) implemented behind injectable traits but only for Windows. Porting to Linux (canon target Ubuntu on Wayland/GNOME) meant implementing those seams for the platform, choosing a windowing strategy that the overlay UX survives, and packaging, without disturbing the Windows build or the behavioural goldens.

## Context and problem statement

The backend member crates already built and passed on Linux CI (they carry no GUI dependency by design); the port gap was the Tauri shell, the four platform seams, the windowing model, and packaging. Each seam had a Windows implementation behind a trait and an inert non-Windows stub; the port fills the stub for Linux under `cfg(target_os = "linux")` rather than refactoring any call site. Three questions had no default answer on Wayland:

- **Global key observation.** The listener passively observes a scan hotkey and hotbar weapon-switches while the game keeps focus. Wayland's security model gives a client no passive view of keys it is not focused on, and the `GlobalShortcuts` portal intercepts a chord rather than observing it (it would steal the key from the game). There is no portal for passive observation.
- **Screen capture.** The Windows path grabs any screen rectangle on demand through GDI, with no prior consent. Wayland forbids that: a client may only capture through a consented portal stream.
- **Windowing.** The overlay and its satellite popups position themselves at absolute screen coordinates and hold always-on-top. Native Wayland denies a client both: it cannot read or set its own global position, and there is no layer-shell on the canon compositor.

## Decision

- **Windowing rides XWayland.** The Linux shell forces the GTK/webview stack onto the X11 backend at startup (before any GTK initialisation), so both the app's windows and the game (which runs under Proton, itself rendering through XWayland) live on the X server, where absolute positioning and always-on-top are honoured and windows are enumerable. An explicit operator backend choice is respected; the default is the backend the overlay UX actually works on. A Wayland-native satellite rework (in-window layers with per-region input shaping) is a possible later enhancement, not a port prerequisite.
- **Key observation reads evdev directly.** A passive reader per keyboard-class device reads the kernel `/dev/input` event stream without ever grabbing the device, so the focused application keeps receiving every key. Read access comes from the `input` group. The constructor-passed allowlist filters at the source exactly as on Windows, so out-of-scope keystrokes never enter the event stream.
- **Capture goes through the ScreenCast portal.** A consented PipeWire monitor stream is opened once and held for the process; consent persists through a restore token stored under the app data directory, so later launches acquire the stream silently. Each capture crops the latest frame to the requested rectangle, translating global screen coordinates to monitor-local ones. The seam's per-call contract (a rectangle in, pixels out) is unchanged; only its backing shifted from an on-demand grab to a held stream, because that is what Wayland permits.
- **Game-window discovery walks the X11 client list.** Title-prefix match over `_NET_CLIENT_LIST` with a client-area-to-root geometry translation, the same title-then-client-area contract as the Windows helpers.
- **OCR keeps its committed runtime, platform-forked and GPU-first on both platforms.** The bundled Windows ONNX Runtime is the DirectML build; Linux bundles the official WebGPU-enabled build (Dawn over Vulkan, with the CPU provider alongside), resolved and pinned by absolute path exactly as on Windows. The execution-provider ladder prefers the platform's GPU provider with a CPU retry, preserving the design intent that inference stays off the CPU the game competes for; without a usable Vulkan adapter the session falls back to CPU. WebGPU is the Linux counterpart of DirectML by construction: a vendor-neutral provider riding the Vulkan driver every desktop distribution already ships, rather than a vendor compute stack (CUDA, ROCm) the user would have to install.
- **Packaging produces a `.deb` and an AppImage** through the Tauri bundler, applied through a Linux configuration overlay so the shared configuration stays Windows-focused; a CI job builds and bundles the Linux package on every change so a Linux-only regression fails there rather than at release.

## Consequences

The port is additive behind `cfg(target_os = "linux")`: the Windows build and every behavioural golden are untouched, and the seams are proven on the platform through their production code paths (passive key observation with allowlist filtering, portal capture with silent token re-acquisition, the full skill-scan OCR chain reading real skill rows with identical output under the WebGPU and CPU providers, and an installed package launching). Two hardening items follow from the Wayland windowing reality: the backend now bounds-checks a persisted overlay position so a client that reports a degenerate location (the shape a Wayland backend returns) cannot poison the store, and the richer monitor-geometry-aware guard on the client side is owned by the frontend windowing work. What is deferred: the live-game validation surface (the real chat-log path inside the Proton prefix, capture and input against the running client) belongs to on-hardware acceptance, and the Linux build is labelled experimental until that acceptance passes. Batched multi-cell inference (a throughput gain on top of the per-cell GPU path) remains a possible later addition on both platforms.

See [ADR-0013](0013-in-process-collapse.md) for the single-binary shape the platform layer sits inside, [ADR-0015](0015-candle-ocr-backend-not-adopted.md) for the ONNX Runtime OCR backend this bundles for Linux, and the [ADR index](index.md).

## Evidence

- `app/src-tauri/eo-services/src/keystroke_source.rs` (the evdev reader behind the keystroke-source seam)
- `app/src-tauri/eo-services/src/screen_capture.rs` (the ScreenCast-portal + PipeWire capturer)
- `app/src-tauri/eo-services/src/eu_window.rs` (X11 game-window discovery and geometry)
- `app/src-tauri/entropia-orme/src/lib.rs` (the X11 backend default and capture-token wiring at startup)
- `app/src-tauri/entropia-orme/src/composition.rs` (the platform-forked ONNX Runtime resolution and pin)
- `app/src-tauri/entropia-orme/resources/ort-linux/` (the bundled Linux WebGPU-enabled runtime and its provenance)
- `app/src-tauri/entropia-orme/tauri.linux.conf.json` (the packaging overlay: deb + AppImage targets, the Linux runtime resource)
