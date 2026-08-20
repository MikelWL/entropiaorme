import { describe, expect, it } from 'vitest';
import type { ProtectionOverview } from '$lib/api';
import { buildProtectionCostSteps } from './protectionCostFlow';

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
