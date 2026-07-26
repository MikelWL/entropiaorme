import { describe, expect, it } from 'vitest';
import { IN_DEVELOPMENT_SURFACES, inDevelopmentSurface } from './registry';

describe('in-development registry', () => {
	// An empty register is the healthy resting state: it means no surface is
	// currently shipping ahead of its capability. So this asserts the shape of
	// whatever is registered rather than naming an entry, which would have to
	// be rewritten every time one graduates.
	it('resolves every registered surface to complete copy', () => {
		for (const registered of IN_DEVELOPMENT_SURFACES) {
			const surface = inDevelopmentSurface(registered.id);
			expect(surface.summary).not.toHaveLength(0);
			expect(surface.graduates).not.toHaveLength(0);
		}
	});

	it('throws on an unregistered id so a stray marker fails loudly', () => {
		expect(() => inDevelopmentSurface('not-a-surface')).toThrow(/not registered/);
	});

	it('keeps ids unique', () => {
		const ids = IN_DEVELOPMENT_SURFACES.map((s) => s.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it('keeps register copy in end-user language', () => {
		// This copy renders in the app, so it reads as product text rather than
		// as a developer note left in place.
		const developerNote = /\b(TODO|FIXME|WIP|placeholder|unimplemented|stub|XXX)\b/i;
		for (const surface of IN_DEVELOPMENT_SURFACES) {
			expect(surface.summary).not.toMatch(developerNote);
			expect(surface.graduates).not.toMatch(developerNote);
		}
	});
});
