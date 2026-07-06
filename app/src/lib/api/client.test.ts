import { beforeEach, describe, expect, it, vi } from 'vitest';

// The two hand-written pieces of the client seam that outlive the typed-command
// transport: the `ApiError` thrown across the facade, and the manual-scan
// capture-preview helper (raw image bytes over the `capture_png` command).
// `invoke` is captured at module load, so the mock is hoisted and the module is
// re-imported per test.
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

type Mod = typeof import('./client');

async function loadModule(): Promise<Mod> {
	vi.resetModules();
	return import('./client');
}

beforeEach(() => {
	invokeMock.mockReset();
});

describe('ApiError', () => {
	it('carries kind, message, and a distinguishing name', async () => {
		const { ApiError } = await loadModule();
		const err = new ApiError('conflict', 'teapot');
		expect(err).toBeInstanceOf(Error);
		expect(err.kind).toBe('conflict');
		expect(err.message).toBe('teapot');
		expect(err.name).toBe('ApiError');
	});
});

describe('manualSkillScanCapturePng', () => {
	it('fetches the capture preview over the capture_png command as a base64 data URL', async () => {
		const { manualSkillScanCapturePng } = await loadModule();
		invokeMock.mockResolvedValue('aGVsbG8=');
		const url = await manualSkillScanCapturePng(3);
		expect(invokeMock).toHaveBeenCalledWith('capture_png', { page: 3 });
		expect(url).toBe('data:image/png;base64,aGVsbG8=');
	});
});
