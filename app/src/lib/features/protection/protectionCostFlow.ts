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
	return buildProtectionCostStepsForLoadout(overview, active.id);
}

export function buildProtectionCostStepsForLoadout(
	overview: ProtectionOverview,
	loadoutId: string,
): ProtectionCostStep[] {
	const active = overview.loadouts.find((loadout) => loadout.id === loadoutId);
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

/** The layer a step measures, in the product's own words. */
export function protectionCostLayerLabel(layer: ProtectionCostLayer): string {
	if (layer === 'armour') return 'Armour';
	if (layer === 'plates') return 'Plates';
	return 'Armour + plates';
}

/** What the user has to do at the terminal before a step can be read. */
export function protectionCostInstruction(step: ProtectionCostStep): string {
	if (step.layer === 'combined') {
		return 'Place all equipped armour and plates in the Repair Terminal.';
	}
	const items = step.layer === 'armour' ? 'seven armour pieces' : 'seven plates';
	const terminal = step.method === 'limited' ? 'Trade Terminal' : 'Repair Terminal';
	return `Place the ${items} in the ${terminal}. Do not complete the transaction.`;
}

/** Idempotency token for one step's confirmation. */
export function protectionCostClientToken(index: number): string {
	return (
		globalThis.crypto?.randomUUID?.() ??
		`protection-cost-${Date.now()}-${index}-${Math.random().toString(36).slice(2)}`
	);
}

export interface ProtectionCostAction {
	/** Whether pressing the control can actually start a recording flow. */
	enabled: boolean;
	/** What the control says it will do, matching the flow that will run. */
	label: string;
}

/**
 * What the armour-cost control offers right now, for the attribution the
 * session was stamped with. Whole-session attribution asks which composed
 * setup was worn, so it is available whenever a setup exists, whatever the
 * live selection is; per-segment attribution follows the active loadout's
 * steps. A catalogue with no setups at all keeps the generic combined
 * repair reading, which needs no composition.
 */
export function protectionCostAction(
	overview: ProtectionOverview | null,
	bySegment: boolean,
): ProtectionCostAction {
	if (!bySegment && (overview?.loadouts.length ?? 0) > 0) {
		return { enabled: true, label: 'Record armour cost' };
	}
	return {
		enabled: buildProtectionCostSteps(overview).length > 0,
		label: protectionCostActionLabel(overview),
	};
}

export function protectionCostActionLabel(overview: ProtectionOverview | null): string {
	if (
		overview &&
		overview.loadouts.length > 0 &&
		!overview.loadouts.some((loadout) => loadout.id === overview.activeLoadoutId)
	) {
		return 'Select an armour loadout first';
	}
	const steps = buildProtectionCostSteps(overview);
	if (steps.length === 0) return 'No armour cost to record';
	if (steps.length === 1 && steps[0].method === 'repair') return 'Record repair cost';
	return `Record ${steps.length} armour ${steps.length === 1 ? 'cost' : 'costs'}`;
}
