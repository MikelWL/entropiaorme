// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import HuntingSessionActivities from './HuntingSessionActivities.svelte';
import type { HuntingActivitySection } from './huntingModel.svelte';
import { marketOpportunity } from './treeCuttingModel.svelte';

function activity(overrides: Partial<HuntingActivitySection> = {}): HuntingActivitySection {
	return {
		kind: 'quest',
		label: 'Daily Hunting 1',
		cycled: 100,
		returns: 90,
		lootRate: 0.9,
		confirmedRewardPed: 15,
		rewardedReturns: 105,
		rewardedRate: 1.05,
		rewardStatus: 'fixed_liquid',
		lootItems: [],
		key: 'quest:daily-1',
		isUnscoped: false,
		muProjectedReturns: 104,
		muRewardedReturns: 119,
		muRewardedRate: 1.19,
		items: [
			{
				name: 'Animal Muscle Oil',
				quantity: 40,
				ttValue: 90,
				sharePct: 100,
				opportunity: marketOpportunity(undefined, 100.6),
				ownMarkupPct: 120,
				markupHorizon: 'week',
				effectiveMarkupPct: 120,
				floored: false,
				tier: 'liquid',
				salesPed: 5000,
				weeklySalesPed: 5000,
			},
		],
		variants: [],
		...overrides,
	};
}

describe('HuntingSessionActivities', () => {
	it('shows the base-to-rewarded decision in the compact list and its exact equation on drill-in', async () => {
		render(HuntingSessionActivities, {
			props: { activities: [activity()], marketAvailable: true },
		});

		expect(screen.getByText('90.0%')).not.toBeNull();
		expect(screen.getByText('105.0%')).not.toBeNull();
		await fireEvent.click(screen.getByRole('button', { name: /Daily Hunting 1/ }));
		expect(screen.getByText('TT Net')).not.toBeNull();
		expect(screen.getByText('Rewarded Net')).not.toBeNull();
		expect(screen.getByText(/90.00 loot \+ 15.00 reward/)).not.toBeNull();
		expect(screen.getByText('Animal Muscle Oil')).not.toBeNull();
	});

	it('labels tracked completion loot without adding it again', async () => {
		render(HuntingSessionActivities, {
			props: {
				activities: [
					activity({
						returns: 105,
						lootRate: 1.05,
						confirmedRewardPed: 0,
						rewardedReturns: 105,
						rewardedRate: 1.05,
						rewardStatus: 'included_in_loot',
					}),
				],
				marketAvailable: true,
			},
		});

		expect(screen.getByText('Included in loot')).not.toBeNull();
		expect(screen.getAllByText('105.0%')).toHaveLength(1);
	});

	it('suppresses an unverified historical reward instead of reading current quest settings', async () => {
		render(HuntingSessionActivities, {
			props: {
				activities: [activity({ rewardStatus: 'unverified', key: 'quest:legacy' })],
				marketAvailable: false,
			},
		});

		expect(screen.getByText('Earlier reward unverified')).not.toBeNull();
		expect(screen.getByText('—')).not.toBeNull();
	});
});
