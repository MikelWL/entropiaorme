import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Read a port from env, validate range, fall back to a default when unset.
// Fails fast at config time with a descriptive error so an invalid value
// surfaces during `vite` startup rather than producing NaN binds or
// malformed URLs in the resulting bundle. The dev-config overlay writer
// (build-dev-config.mjs) enforces the same contract on its side.
function readPort(name: string, defaultValue: number): number {
	const raw = (process.env[name] ?? String(defaultValue)).trim();
	const port = Number(raw);
	if (!Number.isInteger(port) || port < 1 || port > 65535) {
		throw new Error(`${name} must be an integer between 1 and 65535`);
	}
	return port;
}

// Frontend port: bound by Vite's dev server. Process env is available here
// because just sources .env.local before invoking vite.
const port = readPort('ENTROPIAORME_FRONTEND_PORT', 5173);

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	server: {
		port,
		strictPort: true,
		// The shell pre-spawns its hidden satellite windows (overlay,
		// scan-overlay) at boot, so their routes are requested while the dev
		// server is still cold. Under that parallel first load, a component's
		// virtual CSS module can be requested before the component itself has
		// compiled; the raw .svelte file then falls through to the CSS
		// pipeline and the request 500s, leaving that window's webview on a
		// dead document until reload. Pre-transforming every component closes
		// the gap for whichever route a window requests first; the cost is a
		// few seconds of background compile at dev-server start.
		warmup: {
			clientFiles: ['./src/routes/**/*.svelte', './src/lib/**/*.svelte'],
		},
	},
	define: {
		// Forces the JS-driven chart tweens to settle instantly (visual-regression
		// determinism). Set to '1' only by the e2e's own Vite build; unset (so '')
		// in every shipped build, where the freeze branch then folds to a static
		// false and tree-shakes out. See app/src/lib/motion/testMotion.ts.
		'import.meta.env.E2E_FREEZE_TWEENS': JSON.stringify(process.env.E2E_FREEZE_TWEENS ?? ''),
	},
});
