// Writes a per-checkout Tauri config overlay carrying the dev devUrl.
// `tauri dev` is invoked with `--config tauri.dev.conf.json --config
// tauri.dev.local.json` so the two overlays merge over the base via
// Tauri's JSON-merge-patch config-extension mechanism.
//
// The devUrl is `http://localhost:<port>` driven by
// ENTROPIAORME_FRONTEND_PORT (default 5173), matching the port Vite
// binds (app/vite.config.ts reads the same variable). Parallel
// checkouts of this repo coexist on one machine by giving each a
// distinct port (and data directory) in its own `.env.local`.
//
// The indirection exists because Tauri 2 does not support `${env:VAR}`
// interpolation inside tauri.conf.json field values, and the only
// dev-URL-related environment variable it reads (TAURI_DEV_HOST) targets
// mobile public-network development rather than overriding devUrl. The
// generated overlay is the smallest portable shim that keeps the env-driven
// devUrl honoured by Tauri's webview-loading side without hardcoding values
// in committed config.
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { argv, env, exit } from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Read and validate ENTROPIAORME_FRONTEND_PORT. Fails fast with a
// descriptive error so an invalid value surfaces here rather than as a
// malformed devUrl at launch.
export function readFrontendPort() {
	const rawPort = (env.ENTROPIAORME_FRONTEND_PORT ?? '5173').trim();
	const port = Number(rawPort);
	if (!Number.isInteger(port) || port < 1 || port > 65535) {
		throw new Error('ENTROPIAORME_FRONTEND_PORT must be an integer between 1 and 65535');
	}
	return port;
}

// Pure mapping from the validated port to the overlay's devUrl, exported
// for the unit tests.
export function devUrlForPort(port) {
	return `http://localhost:${port}`;
}

function main() {
	const overlay = { build: { devUrl: devUrlForPort(readFrontendPort()) } };
	const out =
		(env.ENTROPIAORME_DEVCONFIG_OUT ?? '').trim() ||
		join(dirname(fileURLToPath(import.meta.url)), 'tauri.dev.local.json');
	writeFileSync(out, `${JSON.stringify(overlay, null, 2)}\n`);
}

// Run only when executed directly (`node build-dev-config.mjs`), not when
// imported by the unit tests.
if (argv[1] && import.meta.url === pathToFileURL(argv[1]).href) {
	try {
		main();
	} catch (err) {
		console.error(err.message);
		exit(1);
	}
}
