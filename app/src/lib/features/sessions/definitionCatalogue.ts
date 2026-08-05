import type { SessionDefinition } from '$lib/api';

/** One catalogue order everywhere definitions are offered: readable,
 * case-insensitive, numeric-aware, and deterministic for equal names. */
export function compareDefinitions(a: SessionDefinition, b: SessionDefinition): number {
	return (
		a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true }) ||
		a.id.localeCompare(b.id, undefined, { numeric: true })
	);
}

export function sortDefinitions(definitions: SessionDefinition[]): SessionDefinition[] {
	return definitions.toSorted(compareDefinitions);
}

export function filterDefinitions(
	definitions: SessionDefinition[],
	query: string,
): SessionDefinition[] {
	const needle = query.trim().toLocaleLowerCase();
	if (!needle) return definitions;
	return definitions.filter((definition) => definition.name.toLocaleLowerCase().includes(needle));
}
