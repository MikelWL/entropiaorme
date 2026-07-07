import { describe, expect, it } from 'vitest';
import {
	formatCooldownHours,
	formatMinutes,
	getCooldownRemaining,
	getCooldownStatus,
} from './cooldown';

// `now` is a plain parameter, so no fake timers: everything is fixed instants.
const NOW = Date.parse('2026-07-07T12:00:00Z');

/** An expiry `deltaMs` away from NOW, as the ISO string the wire carries. */
const expiresIn = (deltaMs: number): string => new Date(NOW + deltaMs).toISOString();

describe('getCooldownStatus', () => {
	it('classifies a quest without a cooldown duration as no_cooldown', () => {
		expect(getCooldownStatus({ cooldownDurationHours: null, cooldownExpiresAt: null }, NOW)).toBe(
			'no_cooldown',
		);
	});

	it('treats a zero-hour duration as no cooldown', () => {
		expect(
			getCooldownStatus({ cooldownDurationHours: 0, cooldownExpiresAt: expiresIn(1000) }, NOW),
		).toBe('no_cooldown');
	});

	it('is ready when a cooldown exists but no expiry is set', () => {
		expect(getCooldownStatus({ cooldownDurationHours: 21, cooldownExpiresAt: null }, NOW)).toBe(
			'ready',
		);
	});

	it('is cooling strictly before the expiry instant', () => {
		expect(
			getCooldownStatus({ cooldownDurationHours: 21, cooldownExpiresAt: expiresIn(1) }, NOW),
		).toBe('cooling');
	});

	it('flips to ready exactly at the expiry instant', () => {
		expect(
			getCooldownStatus({ cooldownDurationHours: 21, cooldownExpiresAt: expiresIn(0) }, NOW),
		).toBe('ready');
	});

	it('is ready after the expiry instant', () => {
		expect(
			getCooldownStatus({ cooldownDurationHours: 21, cooldownExpiresAt: expiresIn(-1) }, NOW),
		).toBe('ready');
	});
});

describe('getCooldownRemaining', () => {
	const quest = (deltaMs: number | null) => ({
		cooldownDurationHours: 21,
		cooldownExpiresAt: deltaMs === null ? null : expiresIn(deltaMs),
	});

	it('returns null without an expiry', () => {
		expect(getCooldownRemaining(quest(null), NOW)).toBeNull();
	});

	it('returns null at and past the expiry instant', () => {
		expect(getCooldownRemaining(quest(0), NOW)).toBeNull();
		expect(getCooldownRemaining(quest(-5000), NOW)).toBeNull();
	});

	it('formats day-scale remainders as Nd Nh', () => {
		const twoDaysFiveHours = (2 * 24 + 5) * 3600 * 1000;
		expect(getCooldownRemaining(quest(twoDaysFiveHours), NOW)).toBe('2d 5h');
	});

	it('formats hour-scale remainders as Nh MMm with zero-padded minutes', () => {
		const threeHoursSevenMinutes = (3 * 3600 + 7 * 60) * 1000;
		expect(getCooldownRemaining(quest(threeHoursSevenMinutes), NOW)).toBe('3h 07m');
	});

	it('formats minute-scale remainders as Nm SSs with zero-padded seconds', () => {
		const fiveMinutesThreeSeconds = (5 * 60 + 3) * 1000;
		expect(getCooldownRemaining(quest(fiveMinutesThreeSeconds), NOW)).toBe('5m 03s');
	});

	it('floors sub-second remainders to whole seconds', () => {
		expect(getCooldownRemaining(quest(1500), NOW)).toBe('0m 01s');
	});
});

describe('formatCooldownHours', () => {
	it('renders whole-day multiples in days', () => {
		expect(formatCooldownHours(24)).toBe('1d');
		expect(formatCooldownHours(168)).toBe('7d');
	});

	it('renders everything else in hours, including above a day', () => {
		expect(formatCooldownHours(21)).toBe('21h');
		expect(formatCooldownHours(36)).toBe('36h');
	});
});

describe('formatMinutes', () => {
	it('renders under an hour in minutes', () => {
		expect(formatMinutes(45)).toBe('45m');
	});

	it('renders whole hours without a minutes part', () => {
		expect(formatMinutes(60)).toBe('1h');
		expect(formatMinutes(120)).toBe('2h');
	});

	it('renders mixed durations as Nh Nm', () => {
		expect(formatMinutes(90)).toBe('1h 30m');
	});
});
