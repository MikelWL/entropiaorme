/**
 * Quest cooldown derivations and duration formatting. Pure: the wall-clock
 * instant is always passed in as `now` (epoch milliseconds), so the functions
 * are directly testable and the caller owns the ticking.
 *
 * Availability has TWO gates: the quest's own cooldown window and its
 * family's window (variants of one repeatable slot cool as a unit). A
 * quest is cooling while EITHER window is open; the two are kept
 * distinguishable because only the OWN window is cancellable from a
 * quest row (a member action must never eat the family's timer).
 */

import type { Quest } from '$lib/types';
import type { CooldownStatus } from '$lib/types/common';

/** The quest fields the cooldown derivations read. */
export type CooldownFields = Pick<
	Quest,
	| 'cooldownDurationHours'
	| 'cooldownExpiresAt'
	| 'familyCooldownDurationHours'
	| 'familyCooldownExpiresAt'
>;

/** A family's own availability fields (the families management list). */
export interface FamilyCooldownFields {
	cooldownDurationHours: number | null;
	cooldownExpiresAt: string | null;
}

function expiryMs(expiresAt: string | null): number | null {
	return expiresAt ? new Date(expiresAt).getTime() : null;
}

/** The later of the two windows' expiries (epoch ms), null when neither is set. */
function effectiveExpiryMs(quest: CooldownFields): number | null {
	const own = expiryMs(quest.cooldownExpiresAt);
	const family = expiryMs(quest.familyCooldownExpiresAt);
	if (own == null) return family;
	if (family == null) return own;
	return Math.max(own, family);
}

/** Which gate currently holds the quest: its own window, the family's, or none. */
export function getCooldownGate(quest: CooldownFields, now: number): 'none' | 'own' | 'family' {
	const own = expiryMs(quest.cooldownExpiresAt);
	if (own != null && now < own) return 'own';
	const family = expiryMs(quest.familyCooldownExpiresAt);
	if (family != null && now < family) return 'family';
	return 'none';
}

export function getCooldownStatus(quest: CooldownFields, now: number): CooldownStatus {
	if (!quest.cooldownDurationHours && !quest.familyCooldownDurationHours) return 'no_cooldown';
	const expiry = effectiveExpiryMs(quest);
	if (expiry == null) return 'ready';
	return now >= expiry ? 'ready' : 'cooling';
}

export function getCooldownRemaining(quest: CooldownFields, now: number): string | null {
	const expiry = effectiveExpiryMs(quest);
	if (expiry == null) return null;
	return formatRemaining(expiry - now);
}

/** A family's own status, over its derived expiry (the families list). */
export function getFamilyCooldownStatus(family: FamilyCooldownFields, now: number): CooldownStatus {
	if (!family.cooldownDurationHours) return 'no_cooldown';
	const expiry = expiryMs(family.cooldownExpiresAt);
	if (expiry == null) return 'ready';
	return now >= expiry ? 'ready' : 'cooling';
}

export function getFamilyCooldownRemaining(
	family: FamilyCooldownFields,
	now: number,
): string | null {
	const expiry = expiryMs(family.cooldownExpiresAt);
	if (expiry == null) return null;
	return formatRemaining(expiry - now);
}

function formatRemaining(remainMs: number): string | null {
	if (remainMs <= 0) return null;
	const totalSec = Math.floor(remainMs / 1000);
	const d = Math.floor(totalSec / 86400);
	const h = Math.floor((totalSec % 86400) / 3600);
	const m = Math.floor((totalSec % 3600) / 60);
	const s = totalSec % 60;
	if (d > 0) return `${d}d ${h}h`;
	if (h > 0) return `${h}h ${m.toString().padStart(2, '0')}m`;
	return `${m}m ${s.toString().padStart(2, '0')}s`;
}

export function formatCooldownHours(h: number): string {
	if (h >= 24 && h % 24 === 0) return `${h / 24}d`;
	return `${h}h`;
}

export function formatMinutes(m: number): string {
	if (m < 60) return `${m}m`;
	const h = Math.floor(m / 60);
	const rem = m % 60;
	return rem > 0 ? `${h}h ${rem}m` : `${h}h`;
}
