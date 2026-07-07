/**
 * Quest cooldown derivations and duration formatting. Pure: the wall-clock
 * instant is always passed in as `now` (epoch milliseconds), so the functions
 * are directly testable and the caller owns the ticking.
 */

import type { Quest } from '$lib/types';
import type { CooldownStatus } from '$lib/types/common';

/** The two quest fields the cooldown derivations read. */
export type CooldownFields = Pick<Quest, 'cooldownDurationHours' | 'cooldownExpiresAt'>;

export function getCooldownStatus(quest: CooldownFields, now: number): CooldownStatus {
	if (!quest.cooldownDurationHours) return 'no_cooldown';
	if (!quest.cooldownExpiresAt) return 'ready';
	return now >= new Date(quest.cooldownExpiresAt).getTime() ? 'ready' : 'cooling';
}

export function getCooldownRemaining(quest: CooldownFields, now: number): string | null {
	if (!quest.cooldownExpiresAt) return null;
	const remainMs = new Date(quest.cooldownExpiresAt).getTime() - now;
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
