/**
 * Presentation helpers for a pin's harvest-visit state. A confirmed visit
 * puts a tree on cooldown; the pin read carries the latest visit time and the
 * server-derived cooldown expiry, so these turn them into hover-card labels.
 */

/** Seconds left on a pin's cooldown, or 0 when it is not on cooldown. */
export function cooldownRemaining(cooldownUntil: number | null, nowSeconds: number): number {
	if (cooldownUntil == null) return 0;
	return Math.max(0, cooldownUntil - nowSeconds);
}

/** A compact duration: `1h 23m`, `12m`, or `45s`. */
export function formatDuration(seconds: number): string {
	const total = Math.max(0, Math.round(seconds));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	if (hours > 0) return `${hours}h ${minutes}m`;
	if (minutes > 0) return `${minutes}m`;
	return `${total}s`;
}

/** `12m left` while a tree is on cooldown, else `null`. */
export function cooldownLabel(cooldownUntil: number | null, nowSeconds: number): string | null {
	const remaining = cooldownRemaining(cooldownUntil, nowSeconds);
	return remaining > 0 ? `${formatDuration(remaining)} left` : null;
}

/** `just now` / `12m ago` for the latest visit, else `null`. */
export function lastVisitedLabel(lastVisitedAt: number | null, nowSeconds: number): string | null {
	if (lastVisitedAt == null) return null;
	const ago = Math.max(0, nowSeconds - lastVisitedAt);
	return ago < 60 ? 'just now' : `${formatDuration(ago)} ago`;
}
