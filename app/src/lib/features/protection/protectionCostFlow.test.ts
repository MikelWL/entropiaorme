import { describe, expect, it } from 'vitest';
import type { ProtectionOverview } from '$lib/api';
import { buildProtectionCostSteps, protectionCostAction } from './protectionCostFlow';

const EMPTY: ProtectionOverview = {
	sets: [],
	loadouts: [],
	activeLoadoutId: null,
	recentReconciliations: [],
	recentCostWindows: [],
};

function overview(
	armour: 'limited' | 'unlimited' | null,
	plates: 'limited' | 'unlimited' | null,
): ProtectionOverview {
	const sets = [
		...(armour
			? [
					{
						id: '1',
						kind: 'armour' as const,
						name: 'Armour',
						economyKind: armour,
						markupPercent: armour === 'limited' ? 125 : null,
						latestObservation: null,
						pendingReconciliations: 0,
						basisLocked: false,
						unsettledDamage: 0,
						unsettledDeflections: 0,
						unsettledSessions: 0,
					},
				]
			: []),
		...(plates
			? [
					{
						id: '2',
						kind: 'plates' as const,
						name: 'Plates',
						economyKind: plates,
						markupPercent: plates === 'limited' ? 140 : null,
						latestObservation: null,
						pendingReconciliations: 0,
						basisLocked: false,
						unsettledDamage: 0,
						unsettledDeflections: 0,
						unsettledSessions: 0,
					},
				]
			: []),
	];
	return {
		sets,
		loadouts: [
			{
				id: 'loadout',
				name: 'Test',
				armour: armour
					? {
							id: '1',
							name: 'Armour',
							economyKind: armour,
							markupPercent: armour === 'limited' ? 125 : null,
						}
					: null,
				plates: plates
					? {
							id: '2',
							name: 'Plates',
							economyKind: plates,
							markupPercent: plates === 'limited' ? 140 : null,
						}
					: null,
			},
		],
		activeLoadoutId: 'loadout',
		recentReconciliations: [],
		recentCostWindows: [],
	};
}

describe('protection cost sequence', () => {
	it('keeps the legacy combined repair step without configured loadouts', () => {
		expect(buildProtectionCostSteps(EMPTY)).toMatchObject([
			{ layer: 'combined', method: 'repair' },
		]);
	});

	it('collapses two unlimited layers into one combined repair reading', () => {
		expect(buildProtectionCostSteps(overview('unlimited', 'unlimited'))).toMatchObject([
			{ layer: 'combined', method: 'repair' },
		]);
	});

	it('orders a mixed loadout as armour then plates', () => {
		expect(buildProtectionCostSteps(overview('unlimited', 'limited'))).toMatchObject([
			{ layer: 'armour', method: 'repair' },
			{ layer: 'plates', method: 'limited' },
		]);
	});

	it('keeps two limited observations separate and armour-first', () => {
		expect(buildProtectionCostSteps(overview('limited', 'limited'))).toMatchObject([
			{ layer: 'armour', method: 'limited' },
			{ layer: 'plates', method: 'limited' },
		]);
	});

	it('has no cost action for the explicit no-protection loadout', () => {
		expect(buildProtectionCostSteps(overview(null, null))).toEqual([]);
	});
});

describe('armour cost control', () => {
	it('offers the whole-session flow whenever a setup exists, whatever is selected', () => {
		const unselected = { ...overview('unlimited', null), activeLoadoutId: null };
		// Per-segment attribution follows the active loadout, so it has nothing
		// to read; whole-session attribution asks which setup was worn instead.
		expect(protectionCostAction(unselected, true)).toEqual({
			enabled: false,
			label: 'Select an armour loadout first',
		});
		expect(protectionCostAction(unselected, false)).toEqual({
			enabled: true,
			label: 'Record armour cost',
		});
	});

	it('keeps the generic combined reading when the catalogue holds no setups', () => {
		// Nothing to choose between, so whole-session attribution falls back to
		// the reading that needs no composition rather than refusing the click.
		expect(protectionCostAction(EMPTY, false)).toEqual({
			enabled: true,
			label: 'Record repair cost',
		});
		expect(protectionCostAction(null, false)).toEqual({
			enabled: true,
			label: 'Record repair cost',
		});
	});

	it('follows the active loadout under per-segment attribution', () => {
		expect(protectionCostAction(overview('limited', 'unlimited'), true)).toEqual({
			enabled: true,
			label: 'Record 2 armour costs',
		});
	});
});
