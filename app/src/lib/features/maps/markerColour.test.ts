import { describe, expect, it } from 'vitest';
import {
	DEFAULT_PRECISION_MARKER_COLOUR,
	markerRgba,
	normaliseHex,
	parseHexRgb,
} from './markerColour';

describe('parseHexRgb', () => {
	it('parses a #rrggbb colour', () => {
		expect(parseHexRgb('#38bdf8')).toEqual({ r: 56, g: 189, b: 248 });
	});

	it('accepts a colour without the leading hash and mixed case', () => {
		expect(parseHexRgb('FF8000')).toEqual({ r: 255, g: 128, b: 0 });
	});

	it('falls back to the default sky-blue for unparseable input', () => {
		expect(parseHexRgb('not-a-colour')).toEqual({ r: 56, g: 189, b: 248 });
		expect(parseHexRgb('#fff')).toEqual({ r: 56, g: 189, b: 248 });
	});

	it('parses the default constant back to its components', () => {
		expect(parseHexRgb(DEFAULT_PRECISION_MARKER_COLOUR)).toEqual({ r: 56, g: 189, b: 248 });
	});
});

describe('normaliseHex', () => {
	it('canonicalises to lowercase #rrggbb', () => {
		expect(normaliseHex('FF8000')).toBe('#ff8000');
		expect(normaliseHex('#38BDF8')).toBe('#38bdf8');
	});

	it('canonicalises bad input to the default', () => {
		expect(normaliseHex('garbage')).toBe('#38bdf8');
	});
});

describe('markerRgba', () => {
	it('builds an rgba string preserving the alpha for additive compositing', () => {
		expect(markerRgba('#38bdf8', 0.92)).toBe('rgba(56, 189, 248, 0.92)');
		expect(markerRgba('#ff8000', 0.5)).toBe('rgba(255, 128, 0, 0.5)');
	});

	it('clamps the alpha into the [0, 1] range', () => {
		expect(markerRgba('#38bdf8', 1.4)).toBe('rgba(56, 189, 248, 1)');
		expect(markerRgba('#38bdf8', -0.2)).toBe('rgba(56, 189, 248, 0)');
	});
});
