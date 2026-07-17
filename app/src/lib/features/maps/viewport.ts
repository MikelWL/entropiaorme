/**
 * The map viewport: one pure transform (zoom + pan) every consumer of
 * the map view renders through (the raster draw, the pin layer, and any
 * future overlay geometry). `zoom` is screen pixels per native image
 * pixel; `panX`/`panY` are the image-pixel coordinates sitting at the
 * view's top-left corner.
 *
 * Invariants the tests pin:
 * - zooming about a cursor keeps the image point under the cursor fixed;
 * - pan is clamped so the image never drifts fully out of view (a
 *   smaller-than-view image centres on the slack axis instead);
 * - zoom is bounded per map: fit-to-view at the bottom, a capped
 *   native-pixel upscale at the top so low-resolution maps stop before
 *   they degrade into blur.
 */

export interface Viewport {
	zoom: number;
	panX: number;
	panY: number;
}

/** The upscale ceiling: screen pixels per native image pixel. */
export const MAX_NATIVE_UPSCALE = 2.5;

/** The zoom multiplier of one wheel/keyboard zoom step. */
export const ZOOM_STEP = 1.2;

/** The zoom that letterboxes the whole image inside the view. */
export function fitZoom(imgW: number, imgH: number, viewW: number, viewH: number): number {
	if (imgW <= 0 || imgH <= 0 || viewW <= 0 || viewH <= 0) return 1;
	return Math.min(viewW / imgW, viewH / imgH);
}

/** The per-map zoom bounds: fit at the bottom (never above the cap),
 * the native-upscale cap at the top. */
export function zoomBounds(
	imgW: number,
	imgH: number,
	viewW: number,
	viewH: number,
): { min: number; max: number } {
	const fit = fitZoom(imgW, imgH, viewW, viewH);
	return { min: Math.min(fit, MAX_NATIVE_UPSCALE), max: MAX_NATIVE_UPSCALE };
}

/** The whole image centred at fit zoom. */
export function fitViewport(imgW: number, imgH: number, viewW: number, viewH: number): Viewport {
	const zoom = zoomBounds(imgW, imgH, viewW, viewH).min;
	return clampPan({ zoom, panX: 0, panY: 0 }, imgW, imgH, viewW, viewH);
}

/** Image pixel -> view (screen) pixel. */
export function imageToView(vp: Viewport, x: number, y: number): { x: number; y: number } {
	return { x: (x - vp.panX) * vp.zoom, y: (y - vp.panY) * vp.zoom };
}

/** View (screen) pixel -> image pixel. */
export function viewToImage(vp: Viewport, x: number, y: number): { x: number; y: number } {
	return { x: x / vp.zoom + vp.panX, y: y / vp.zoom + vp.panY };
}

/** Pan by a screen-pixel delta (drag), clamped. */
export function panBy(
	vp: Viewport,
	dxView: number,
	dyView: number,
	imgW: number,
	imgH: number,
	viewW: number,
	viewH: number,
): Viewport {
	return clampPan(
		{ ...vp, panX: vp.panX - dxView / vp.zoom, panY: vp.panY - dyView / vp.zoom },
		imgW,
		imgH,
		viewW,
		viewH,
	);
}

/**
 * Zoom by `factor` about a view-pixel anchor: the image point under the
 * anchor stays under it (the invariant that makes wheel zoom feel
 * rooted), then the result clamps to the map's zoom and pan bounds.
 */
export function zoomAt(
	vp: Viewport,
	factor: number,
	anchorViewX: number,
	anchorViewY: number,
	imgW: number,
	imgH: number,
	viewW: number,
	viewH: number,
): Viewport {
	const bounds = zoomBounds(imgW, imgH, viewW, viewH);
	const zoom = Math.min(bounds.max, Math.max(bounds.min, vp.zoom * factor));
	const anchor = viewToImage(vp, anchorViewX, anchorViewY);
	const next = {
		zoom,
		panX: anchor.x - anchorViewX / zoom,
		panY: anchor.y - anchorViewY / zoom,
	};
	return clampPan(next, imgW, imgH, viewW, viewH);
}

/**
 * Clamp the pan so the image stays in view: on an axis where the scaled
 * image exceeds the view, panning stops at the image edges; on an axis
 * where it fits, the image centres.
 */
export function clampPan(
	vp: Viewport,
	imgW: number,
	imgH: number,
	viewW: number,
	viewH: number,
): Viewport {
	const clampAxis = (pan: number, img: number, view: number): number => {
		const viewInImage = view / vp.zoom;
		if (img <= viewInImage) return (img - viewInImage) / 2;
		return Math.min(Math.max(pan, 0), img - viewInImage);
	};
	return {
		zoom: vp.zoom,
		panX: clampAxis(vp.panX, imgW, viewW),
		panY: clampAxis(vp.panY, imgH, viewH),
	};
}
