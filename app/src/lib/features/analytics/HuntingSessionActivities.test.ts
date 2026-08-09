// @vitest-environment happy-dom

import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { NO_DATA } from '$lib/utils/format';
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
	it('opens the first activity detail and switches through the searchable economic picker', async () => {
		const alternative = activity({
			label: 'Bonus Challenge',
			key: 'quest:bonus',
			cycled: 80,
			returns: 76,
			lootRate: 0.95,
			confirmedRewardPed: 0,
			rewardedReturns: 76,
			rewardedRate: 0.95,
			rewardStatus: 'none',
		});
		render(HuntingSessionActivities, {
			props: { activities: [activity(), alternative], marketAvailable: true },
		});

		const trigger = screen.getByLabelText('Switch session activity (currently Daily Hunting 1)');
		expect(screen.getByText('TT Net')).not.toBeNull();
		expect(screen.getByText('Rewarded Net')).not.toBeNull();
		expect(screen.getByText(/90.00 loot \+ 15.00 reward/)).not.toBeNull();
		expect(screen.getByText('Animal Muscle Oil')).not.toBeNull();
		expect(screen.queryByRole('menu')).toBeNull();

		await fireEvent.keyDown(trigger, { key: 'ArrowDown' });
		const filter = screen.getByLabelText('Filter session activities');
		expect(document.activeElement).toBe(filter);
		const menu = screen.getByRole('menu');
		for (const label of ['Cycled', 'TT Rate', 'Rewarded Rate']) {
			expect(within(menu).getByText(label)).not.toBeNull();
		}
		expect(within(menu).getByText('90.0%')).not.toBeNull();
		expect(within(menu).getByText('105.0%')).not.toBeNull();

		await fireEvent.input(filter, { target: { value: 'bonus' } });
		const match = screen.getByRole('menuitem', { name: /Bonus Challenge/ });
		expect(screen.queryByRole('menuitem', { name: /Daily Hunting 1/ })).toBeNull();
		await fireEvent.keyDown(filter, { key: 'ArrowDown' });
		expect(document.activeElement).toBe(match);
		await fireEvent.click(match);

		expect(
			screen.getByLabelText('Switch session activity (currently Bonus Challenge)'),
		).not.toBeNull();
		expect(screen.queryByRole('menu')).toBeNull();
	});

	it('labels tracked completion loot without adding it again', () => {
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
		for (const value of screen.getAllByText('+5.00')) {
			expect(value.classList.contains('text-positive')).toBe(false);
		}
	});

	it('suppresses an unverified historical reward instead of reading current quest settings', () => {
		render(HuntingSessionActivities, {
			props: {
				activities: [activity({ rewardStatus: 'unverified', key: 'quest:legacy' })],
				marketAvailable: false,
			},
		});

		expect(screen.getByText('Earlier reward unverified')).not.toBeNull();
		expect(screen.getAllByText(NO_DATA).length).toBeGreaterThan(0);
	});

	it('keeps variants in the picker hierarchy and opens their detail directly', async () => {
		const variant = activity({
			label: 'Daily Hunting 1 + Bonus',
			key: 'quest:daily-1/variant:bonus',
			variants: [],
		});
		render(HuntingSessionActivities, {
			props: {
				activities: [activity({ variants: [variant] })],
				marketAvailable: true,
			},
		});

		await fireEvent.click(
			screen.getByLabelText('Switch session activity (currently Daily Hunting 1)'),
		);
		const variantRow = screen.getByRole('menuitem', { name: /Daily Hunting 1 \+ Bonus/ });
		expect(variantRow.textContent).toContain('↳');
		await fireEvent.click(variantRow);
		expect(
			screen.getByLabelText('Switch session activity (currently Daily Hunting 1 + Bonus)'),
		).not.toBeNull();
	});

	it('falls back to the first declared activity when the selected session changes', async () => {
		const firstSession = [activity(), activity({ label: 'Bonus Challenge', key: 'quest:bonus' })];
		const view = render(HuntingSessionActivities, {
			props: { activities: firstSession, marketAvailable: true },
		});

		await fireEvent.click(
			screen.getByLabelText('Switch session activity (currently Daily Hunting 1)'),
		);
		await fireEvent.click(screen.getByRole('menuitem', { name: /Bonus Challenge/ }));
		expect(
			screen.getByLabelText('Switch session activity (currently Bonus Challenge)'),
		).not.toBeNull();

		await view.rerender({
			activities: [activity({ label: 'New Session Activity', key: 'quest:new-session' })],
			marketAvailable: true,
		});
		expect(
			screen.getByLabelText('Switch session activity (currently New Session Activity)'),
		).not.toBeNull();
	});
});
