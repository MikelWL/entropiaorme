import type { ProtectionOverview, ProtectionSetRef } from '$lib/api';

export type ProtectionCostLayer = 'armour' | 'plates' | 'combined';
export type ProtectionCostMethod = 'repair' | 'limited';

export interface ProtectionCostStep {
	layer: ProtectionCostLayer;
	method: ProtectionCostMethod;
	name: string;
	setId: string | null;
	armourSetId: string | null;
	plateSetId: string | null;
	markupPercent: number | null;
	baselineTtPed: number | null;
}

function componentStep(
	overview: ProtectionOverview,
	layer: 'armour' | 'plates',
	component: ProtectionSetRef,
): ProtectionCostStep {
	const set = overview.sets.find((candidate) => candidate.id === component.id);
	return {
		layer,
		method: component.economyKind === 'limited' ? 'limited' : 'repair',
		name: component.name,
		setId: component.id,
		armourSetId: layer === 'armour' ? component.id : null,
		plateSetId: layer === 'plates' ? component.id : null,
		markupPercent: component.markupPercent,
		baselineTtPed: set?.latestObservation?.ttValuePed ?? null,
	};
}

export function buildProtectionCostSteps(
	overview: ProtectionOverview | null,
): ProtectionCostStep[] {
	if (!overview || overview.loadouts.length === 0) {
		return [
			{
				layer: 'combined',
				method: 'repair',
				name: 'Armour and plates',
				setId: null,
				armourSetId: null,
				plateSetId: null,
				markupPercent: null,
				baselineTtPed: null,
			},
		];
	}

	const active = overview.loadouts.find((loadout) => loadout.id === overview.activeLoadoutId);
	if (!active) return [];

	const components: ProtectionCostStep[] = [];
	if (active.armour) components.push(componentStep(overview, 'armour', active.armour));
	if (active.plates) components.push(componentStep(overview, 'plates', active.plates));

	if (components.length === 2 && components.every((component) => component.method === 'repair')) {
		return [
			{
				layer: 'combined',
				method: 'repair',
				name: `${active.armour?.name} + ${active.plates?.name}`,
				setId: null,
				armourSetId: active.armour?.id ?? null,
				plateSetId: active.plates?.id ?? null,
				markupPercent: null,
				baselineTtPed: null,
			},
		];
	}

	return components;
}

export function protectionCostActionLabel(overview: ProtectionOverview | null): string {
	if (
		overview &&
		overview.loadouts.length > 0 &&
		!overview.loadouts.some((loadout) => loadout.id === overview.activeLoadoutId)
	) {
		return 'Select a protection loadout first';
	}
	const steps = buildProtectionCostSteps(overview);
	if (steps.length === 0) return 'No protection cost to record';
	if (steps.length === 1 && steps[0].method === 'repair') return 'Record repair cost';
	return `Record ${steps.length} protection ${steps.length === 1 ? 'cost' : 'costs'}`;
}
