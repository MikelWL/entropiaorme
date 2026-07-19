import { describe, expect, it } from 'vitest';
import { cooldownLabel, cooldownRemaining, formatDuration, lastVisitedLabel } from './cooldown';

describe('cooldownRemaining', () => {
	it('is zero with no cooldown', () => {
		expect(cooldownRemaining(null, 1000)).toBe(0);
	});

	it('is the gap while the expiry is ahead of now', () => {
		expect(cooldownRemaining(1500, 1000)).toBe(500);
	});

	it('is zero once the expiry has passed', () => {
		expect(cooldownRemaining(900, 1000)).toBe(0);
	});
});

describe('formatDuration', () => {
	it('formats hours and minutes', () => {
		expect(formatDuration(3600 + 23 * 60)).toBe('1h 23m');
	});

	it('formats minutes only', () => {
		expect(formatDuration(12 * 60)).toBe('12m');
	});

	it('formats seconds under a minute', () => {
		expect(formatDuration(45)).toBe('45s');
	});
});

describe('cooldownLabel', () => {
	it('describes the remaining cooldown', () => {
		expect(cooldownLabel(1000 + 90 * 60, 1000)).toBe('1h 30m left');
	});

	it('is null when off cooldown', () => {
		expect(cooldownLabel(null, 1000)).toBeNull();
		expect(cooldownLabel(500, 1000)).toBeNull();
	});
});

describe('lastVisitedLabel', () => {
	it('is null when never visited', () => {
		expect(lastVisitedLabel(null, 1000)).toBeNull();
	});

	it('reads "just now" within a minute', () => {
		expect(lastVisitedLabel(970, 1000)).toBe('just now');
	});

	it('reads a relative age past a minute', () => {
		expect(lastVisitedLabel(1000 - 12 * 60, 1000)).toBe('12m ago');
	});
});
