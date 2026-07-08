// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from 'vitest';

// Mock the one Tauri seam the sizer owns: the current window's setSize. The
// DOM rect comes from a stubbed element and animation frames are driven by
// hand, so the coalescing and dedupe logic runs deterministically.
const seams = vi.hoisted(() => ({
	setSize: vi.fn(async (_size: unknown) => {}),
}));

vi.mock('@tauri-apps/api/window', () => ({
	getCurrentWindow: () => ({
		setSize: seams.setSize,
	}),
}));

vi.mock('@tauri-apps/api/dpi', () => ({
	LogicalSize: class {
		constructor(
			public width: number,
			public height: number,
		) {}
	},
}));

import { createWindowSizeSync, measureWindowContentSize } from './windowSize';

function fakeRoot(rect: { width: number; height: number }): HTMLElement {
	const el = document.createElement('div');
	el.getBoundingClientRect = () =>
		({
			width: rect.width,
			height: rect.height,
			left: 0,
			top: 0,
			right: rect.width,
			bottom: rect.height,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		}) as DOMRect;
	return el;
}

let pendingFrames: Map<number, FrameRequestCallback>;

async function flushFrames(): Promise<void> {
	const callbacks = [...pendingFrames.values()];
	pendingFrames.clear();
	for (const cb of callbacks) {
		cb(0);
	}
	// Let the async sync (and its afterSync continuation) settle.
	await Promise.resolve();
	await Promise.resolve();
	await Promise.resolve();
}

beforeEach(() => {
	seams.setSize.mockClear();
	seams.setSize.mockResolvedValue(undefined);
	pendingFrames = new Map();
	let nextFrameId = 1;
	vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
		const id = nextFrameId++;
		pendingFrames.set(id, cb);
		return id;
	});
	vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
		pendingFrames.delete(id);
	});
});

describe('measureWindowContentSize', () => {
	it('adds the slack and ceils, so fractional layouts never clip', () => {
		expect(measureWindowContentSize(fakeRoot({ width: 100.2, height: 40.6 }))).toEqual({
			width: 137, // ceil(100.2 + 36)
			height: 77, // ceil(40.6 + 36)
		});
	});
});

describe('createWindowSizeSync', () => {
	it('coalesces a burst of schedules into one frame and applies the measured size', async () => {
		const root = fakeRoot({ width: 100, height: 40 });
		const sizer = createWindowSizeSync(() => root);

		sizer.schedule();
		sizer.schedule();
		sizer.schedule();
		expect(pendingFrames.size).toBe(1);

		await flushFrames();
		expect(seams.setSize).toHaveBeenCalledTimes(1);
		expect(seams.setSize).toHaveBeenCalledWith({ width: 136, height: 76 });
	});

	it('dedupes an unchanged size and re-applies once the content changes', async () => {
		const rect = { width: 100, height: 40 };
		const root = fakeRoot(rect);
		const sizer = createWindowSizeSync(() => root);

		sizer.schedule();
		await flushFrames();
		sizer.schedule();
		await flushFrames();
		expect(seams.setSize).toHaveBeenCalledTimes(1);

		rect.width = 150;
		sizer.schedule();
		await flushFrames();
		expect(seams.setSize).toHaveBeenCalledTimes(2);
		expect(seams.setSize).toHaveBeenLastCalledWith({ width: 186, height: 76 });
	});

	it('forgets the last size when the apply fails, so the next request retries', async () => {
		const root = fakeRoot({ width: 100, height: 40 });
		const sizer = createWindowSizeSync(() => root);
		seams.setSize.mockRejectedValueOnce(new Error('window mid-teardown'));

		sizer.schedule();
		await flushFrames();
		expect(seams.setSize).toHaveBeenCalledTimes(1);

		// The same measurement would dedupe against a size that never landed;
		// the failure must have cleared it.
		sizer.schedule();
		await flushFrames();
		expect(seams.setSize).toHaveBeenCalledTimes(2);
	});

	it('runs afterSync after every scheduled sync, applied or deduped', async () => {
		const afterSync = vi.fn();
		const root = fakeRoot({ width: 100, height: 40 });
		const sizer = createWindowSizeSync(() => root, { afterSync });

		sizer.schedule();
		await flushFrames();
		expect(afterSync).toHaveBeenCalledTimes(1);
		// afterSync follows the apply, never precedes it.
		expect(afterSync.mock.invocationCallOrder[0]).toBeGreaterThan(
			seams.setSize.mock.invocationCallOrder[0],
		);

		sizer.schedule();
		await flushFrames();
		expect(seams.setSize).toHaveBeenCalledTimes(1);
		expect(afterSync).toHaveBeenCalledTimes(2);
	});

	it('no-ops without a root and cancel drops a pending sync', async () => {
		let root: HTMLElement | null = null;
		const sizer = createWindowSizeSync(() => root);

		sizer.schedule();
		expect(pendingFrames.size).toBe(0);

		root = fakeRoot({ width: 100, height: 40 });
		sizer.schedule();
		sizer.cancel();
		expect(pendingFrames.size).toBe(0);
		await flushFrames();
		expect(seams.setSize).not.toHaveBeenCalled();

		// A cancelled sizer schedules again cleanly.
		sizer.schedule();
		await flushFrames();
		expect(seams.setSize).toHaveBeenCalledTimes(1);
	});
});
