/**
 * The register of surfaces that ship ahead of their capability.
 *
 * One register rather than a flag per component, so the set stays auditable
 * and cannot quietly become permanent. Graduating a surface means deleting
 * its entry: the `in-development` guard fails a marker with no entry and an
 * entry with no consumer, so neither half can outlive the other unnoticed.
 *
 * Only genuinely misleading surfaces belong here: a control that does
 * nothing when used, or a figure whose value can diverge from the truth
 * with nothing signalling it. A figure that happens to sit beside unbuilt
 * work but is already correct does not need an entry.
 *
 * Copy is user-facing: what is unavailable, and what will make it work.
 */

export interface InDevelopmentSurface {
	/** Stable key shared by the marker, its consumers, and the guard. */
	readonly id: string;
	readonly summary: string;
	readonly graduates: string;
}

export const IN_DEVELOPMENT_SURFACES: readonly InDevelopmentSurface[] = [
	{
		id: 'session-definition-roster',
		summary:
			'A session saves its activities and whether segments may be named on the fly, but the overlay control that offers them while tracking is not built yet; segments stay free-text for every session meanwhile.',
		graduates:
			'Works once the overlay gains its activity picker, which will offer these entries and honour that choice.',
	},
] as const;

/** The registered surface for `id`. Throws on an unregistered id, matching
 * the guard's build-time check so both enforcement points agree. */
export function inDevelopmentSurface(id: string): InDevelopmentSurface {
	const surface = IN_DEVELOPMENT_SURFACES.find((entry) => entry.id === id);
	if (!surface) {
		throw new Error(`in-development surface "${id}" is not registered`);
	}
	return surface;
}
