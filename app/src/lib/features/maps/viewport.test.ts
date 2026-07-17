import { describe, expect, it } from 'vitest';

import {
	clampPan,
	fitViewport,
	fitZoom,
	imageToView,
	MAX_NATIVE_UPSCALE,
	panBy,
	viewToImage,
	zoomAt,
	zoomBounds,
} from './viewport';

const IMG = { w: 4608, h: 4608 };
const VIEW = { w: 1200, h: 800 };

describe('fit and bounds', () => {
	it('fitZoom letterboxes the constraining axis', () => {
		expect(fitZoom(IMG.w, IMG.h, VIEW.w, VIEW.h)).toBeCloseTo(800 / 4608, 6);
	});

	it('a small map in a large view caps at the native-upscale ceiling', () => {
		const bounds = zoomBounds(512, 512, 1400, 1400);
		expect(bounds.min).toBe(MAX_NATIVE_UPSCALE);
		expect(bounds.max).toBe(MAX_NATIVE_UPSCALE);
	});

	it('fitViewport centres the slack axis', () => {
		const vp = fitViewport(IMG.w, IMG.h, VIEW.w, VIEW.h);
		// Height-constrained: y spans exactly, x centres (negative pan
		// means image origin sits inside the view).
		expect(vp.panY).toBeCloseTo(0, 6);
		const viewWidthInImage = VIEW.w / vp.zoom;
		expect(vp.panX).toBeCloseTo((IMG.w - viewWidthInImage) / 2, 6);
	});
});

describe('zoomAt', () => {
	it('keeps the image point under the anchor fixed once the image overflows the view', () => {
		// Zoom in far enough that the image overflows both axes; while an
		// axis still fits, centring on it deliberately wins over anchoring.
		const overflowing = zoomAt(
			fitViewport(IMG.w, IMG.h, VIEW.w, VIEW.h),
			4,
			600,
			400,
			IMG.w,
			IMG.h,
			VIEW.w,
			VIEW.h,
		);
		const anchor = { x: 700, y: 300 };
		const before = viewToImage(overflowing, anchor.x, anchor.y);
		const zoomed = zoomAt(overflowing, 1.5, anchor.x, anchor.y, IMG.w, IMG.h, VIEW.w, VIEW.h);
		const after = viewToImage(zoomed, anchor.x, anchor.y);
		expect(after.x).toBeCloseTo(before.x, 6);
		expect(after.y).toBeCloseTo(before.y, 6);
	});

	it('clamps to the zoom bounds instead of overshooting', () => {
		const vp = fitViewport(IMG.w, IMG.h, VIEW.w, VIEW.h);
		const maxed = zoomAt(vp, 1e9, 600, 400, IMG.w, IMG.h, VIEW.w, VIEW.h);
		expect(maxed.zoom).toBe(MAX_NATIVE_UPSCALE);
		const minned = zoomAt(vp, 1e-9, 600, 400, IMG.w, IMG.h, VIEW.w, VIEW.h);
		expect(minned.zoom).toBeCloseTo(zoomBounds(IMG.w, IMG.h, VIEW.w, VIEW.h).min, 6);
	});
});

describe('panBy and clampPan', () => {
	it('drags in screen space and stops at the image edges', () => {
		const vp = zoomAt(
			fitViewport(IMG.w, IMG.h, VIEW.w, VIEW.h),
			8,
			600,
			400,
			IMG.w,
			IMG.h,
			VIEW.w,
			VIEW.h,
		);
		const dragged = panBy(vp, -100, -60, IMG.w, IMG.h, VIEW.w, VIEW.h);
		expect(dragged.panX).toBeCloseTo(vp.panX + 100 / vp.zoom, 6);
		expect(dragged.panY).toBeCloseTo(vp.panY + 60 / vp.zoom, 6);

		const slammed = panBy(vp, -1e9, 1e9, IMG.w, IMG.h, VIEW.w, VIEW.h);
		expect(slammed.panX).toBeCloseTo(IMG.w - VIEW.w / vp.zoom, 6);
		expect(slammed.panY).toBeCloseTo(0, 6);
	});

	it('centres an axis where the image fits inside the view', () => {
		const vp = clampPan({ zoom: 1, panX: 50, panY: 50 }, 512, 512, 1200, 800);
		expect(vp.panX).toBeCloseTo((512 - 1200) / 2, 6);
		expect(vp.panY).toBeCloseTo((512 - 800) / 2, 6);
	});
});

describe('imageToView / viewToImage', () => {
	it('are inverses', () => {
		const vp = { zoom: 1.7, panX: 123.4, panY: 56.7 };
		const view = imageToView(vp, 1000, 2000);
		const image = viewToImage(vp, view.x, view.y);
		expect(image.x).toBeCloseTo(1000, 6);
		expect(image.y).toBeCloseTo(2000, 6);
	});
});
