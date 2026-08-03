import { describe, expect, it } from 'vitest';
import {
	type CooldownFields,
	formatCooldownHours,
	formatMinutes,
	getCooldownGate,
	getCooldownRemaining,
	getCooldownStatus,
	getFamilyCooldownRemaining,
	getFamilyCooldownStatus,
} from './cooldown';

// `now` is a plain parameter, so no fake timers: everything is fixed instants.
const NOW = Date.parse('2026-07-07T12:00:00Z');

/** An expiry `deltaMs` away from NOW, as the ISO string the wire carries. */
const expiresIn = (deltaMs: number): string => new Date(NOW + deltaMs).toISOString();

/** A quest's cooldown picture; family fields default to standalone-null. */
const fields = (overrides: Partial<CooldownFields> = {}): CooldownFields => ({
	cooldownDurationHours: null,
	cooldownExpiresAt: null,
	familyCooldownDurationHours: null,
	familyCooldownExpiresAt: null,
	...overrides,
});

describe('getCooldownStatus', () => {
	it('classifies a quest without any cooldown duration as no_cooldown', () => {
		expect(getCooldownStatus(fields(), NOW)).toBe('no_cooldown');
	});

	it('treats a zero-hour duration as no cooldown', () => {
		expect(
			getCooldownStatus(
				fields({ cooldownDurationHours: 0, cooldownExpiresAt: expiresIn(1000) }),
				NOW,
			),
		).toBe('no_cooldown');
	});

	it('is ready when a cooldown exists but no expiry is set', () => {
		expect(getCooldownStatus(fields({ cooldownDurationHours: 21 }), NOW)).toBe('ready');
	});

	it('is cooling strictly before the expiry instant', () => {
		expect(
			getCooldownStatus(
				fields({ cooldownDurationHours: 21, cooldownExpiresAt: expiresIn(1) }),
				NOW,
			),
		).toBe('cooling');
	});

	it('flips to ready exactly at the expiry instant', () => {
		expect(
			getCooldownStatus(
				fields({ cooldownDurationHours: 21, cooldownExpiresAt: expiresIn(0) }),
				NOW,
			),
		).toBe('ready');
	});

	it('is ready after the expiry instant', () => {
		expect(
			getCooldownStatus(
				fields({ cooldownDurationHours: 21, cooldownExpiresAt: expiresIn(-1) }),
				NOW,
			),
		).toBe('ready');
	});

	it('a family window alone makes the quest cooling', () => {
		expect(
			getCooldownStatus(
				fields({ familyCooldownDurationHours: 20, familyCooldownExpiresAt: expiresIn(1000) }),
				NOW,
			),
		).toBe('cooling');
	});

	it('a family duration with no open window reads ready, not no_cooldown', () => {
		expect(getCooldownStatus(fields({ familyCooldownDurationHours: 20 }), NOW)).toBe('ready');
	});

	it('stays cooling while the LATER of the two windows is open', () => {
		expect(
			getCooldownStatus(
				fields({
					cooldownDurationHours: 21,
					cooldownExpiresAt: expiresIn(-1000),
					familyCooldownDurationHours: 20,
					familyCooldownExpiresAt: expiresIn(5000),
				}),
				NOW,
			),
		).toBe('cooling');
	});
});

describe('getCooldownGate', () => {
	it('is none with no open window', () => {
		expect(getCooldownGate(fields({ cooldownDurationHours: 21 }), NOW)).toBe('none');
	});

	it('is own while the quest window is open', () => {
		expect(
			getCooldownGate(
				fields({ cooldownDurationHours: 21, cooldownExpiresAt: expiresIn(1000) }),
				NOW,
			),
		).toBe('own');
	});

	it('is family while only the family window is open', () => {
		expect(
			getCooldownGate(
				fields({
					cooldownDurationHours: 21,
					cooldownExpiresAt: expiresIn(-1000),
					familyCooldownDurationHours: 20,
					familyCooldownExpiresAt: expiresIn(1000),
				}),
				NOW,
			),
		).toBe('family');
	});

	it('reports own when both windows are open (own is the cancellable one)', () => {
		expect(
			getCooldownGate(
				fields({
					cooldownDurationHours: 21,
					cooldownExpiresAt: expiresIn(1000),
					familyCooldownDurationHours: 20,
					familyCooldownExpiresAt: expiresIn(5000),
				}),
				NOW,
			),
		).toBe('own');
	});
});

describe('getCooldownRemaining', () => {
	const quest = (deltaMs: number | null) =>
		fields({
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

	it('counts down the family window when it outlasts the own window', () => {
		const threeHours = 3 * 3600 * 1000;
		expect(
			getCooldownRemaining(
				fields({
					cooldownDurationHours: 21,
					cooldownExpiresAt: expiresIn(1000),
					familyCooldownDurationHours: 20,
					familyCooldownExpiresAt: expiresIn(threeHours),
				}),
				NOW,
			),
		).toBe('3h 00m');
	});
});

describe('family cooldown derivations', () => {
	it('an ungated family is no_cooldown', () => {
		expect(
			getFamilyCooldownStatus({ cooldownDurationHours: null, cooldownExpiresAt: null }, NOW),
		).toBe('no_cooldown');
	});

	it('a gated family with an open window is cooling with a countdown', () => {
		const family = {
			cooldownDurationHours: 20,
			cooldownExpiresAt: expiresIn(60 * 1000),
		};
		expect(getFamilyCooldownStatus(family, NOW)).toBe('cooling');
		expect(getFamilyCooldownRemaining(family, NOW)).toBe('1m 00s');
	});

	it('a gated family past its window is ready', () => {
		expect(
			getFamilyCooldownStatus(
				{ cooldownDurationHours: 20, cooldownExpiresAt: expiresIn(-1) },
				NOW,
			),
		).toBe('ready');
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
