import { execFile } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { env, execPath } from 'node:process';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { describe, expect, it } from 'vitest';
import { devUrlForPort } from './build-dev-config.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const scriptPath = join(here, 'build-dev-config.mjs');
const execFileAsync = promisify(execFile);

describe('devUrlForPort (pure mapping)', () => {
	it('maps the port onto the plain-HTTP localhost devUrl', () => {
		expect(devUrlForPort(5173)).toBe('http://localhost:5173');
		expect(devUrlForPort(5199)).toBe('http://localhost:5199');
	});
});

// Integration: drive the real script end-to-end, reading back the overlay
// it writes. The child is run with async execFile so failures surface as
// a captured exit code rather than a thrown sync error.
async function runScript(extraEnv: Record<string, string | undefined>) {
	const outDir = mkdtempSync(join(tmpdir(), 'eo-devconfig-'));
	const out = join(outDir, 'tauri.dev.local.json');
	const childEnv: Record<string, string | undefined> = {
		...env,
		ENTROPIAORME_DEVCONFIG_OUT: out,
		...extraEnv,
	};
	for (const key of Object.keys(childEnv)) {
		if (childEnv[key] === undefined) delete childEnv[key];
	}
	let status = 0;
	let stderr = '';
	try {
		const r = await execFileAsync(execPath, [scriptPath], {
			encoding: 'utf8',
			env: childEnv,
		});
		stderr = r.stderr;
	} catch (err) {
		const e = err as { code?: number; stderr?: string };
		status = e.code ?? 1;
		stderr = e.stderr ?? '';
	}
	let overlay: { build: { devUrl: string } } | null = null;
	try {
		overlay = JSON.parse(readFileSync(out, 'utf8'));
	} catch {
		overlay = null;
	}
	rmSync(outDir, { recursive: true, force: true });
	return { status, stderr, overlay };
}

describe('build-dev-config.mjs (subprocess integration)', () => {
	it('default port: writes the localhost:5173 overlay', async () => {
		const { status, overlay } = await runScript({ ENTROPIAORME_FRONTEND_PORT: undefined });
		expect(status).toBe(0);
		expect(overlay?.build.devUrl).toBe('http://localhost:5173');
	});

	it('custom port: the overlay honours ENTROPIAORME_FRONTEND_PORT', async () => {
		const { status, overlay } = await runScript({ ENTROPIAORME_FRONTEND_PORT: '5199' });
		expect(status).toBe(0);
		expect(overlay?.build.devUrl).toBe('http://localhost:5199');
	});

	it('invalid port: fails fast naming the variable, writes no overlay', async () => {
		const { status, stderr, overlay } = await runScript({ ENTROPIAORME_FRONTEND_PORT: 'nope' });
		expect(status).toBe(1);
		expect(stderr).toContain('ENTROPIAORME_FRONTEND_PORT');
		expect(overlay).toBeNull();
	});
});
