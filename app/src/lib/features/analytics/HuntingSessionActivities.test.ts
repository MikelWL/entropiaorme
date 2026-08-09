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
		rewardMuPed: 18,
		rewardedReturns: 105,
		rewardedRate: 1.05,
		rewardStatus: 'fixed_liquid',
		lootItems: [],
		key: 'quest:daily-1',
		isUnscoped: false,
		muProjectedReturns: 104,
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
		const activityName = screen.getByTitle('Daily Hunting 1');
		expect(activityName.className).toContain('break-words');
		expect(activityName.className).not.toContain('truncate');
		const grid = screen.getByTestId('activity-economic-grid');
		expect(grid.className).toContain('grid-cols-4');
		for (const label of [
			'Reward',
			'TT Net',
			'MU Net',
			'Realised Net',
			'Reward MU',
			'TT Rate',
			'MU Rate',
			'Realised Rate',
		]) {
			expect(within(grid).getByText(label)).not.toBeNull();
		}
		expect(within(grid).getByText('+15.00')).not.toBeNull();
		expect(within(grid).getByText('18.00')).not.toBeNull();
		expect(within(grid).queryByText('Cycled')).toBeNull();
		expect(within(grid).getByText('+4.00').className).not.toContain('text-positive');
		expect(within(grid).getByText('+5.00').className).toContain('text-positive');
		expect(within(grid).getByText('104.0%').className).not.toContain('text-positive');
		expect(within(grid).getByText('105.0%').className).toContain('text-positive');
		expect(screen.queryByText('Rewarded Net')).toBeNull();
		expect(screen.queryByText('At current market')).toBeNull();
		expect(screen.queryByText('Animal Muscle Oil')).toBeNull();
		expect(screen.queryByLabelText('Find an item')).toBeNull();
		const lootTrigger = screen.getByRole('button', { name: 'Show activity loot' });
		expect(lootTrigger.parentElement?.className).not.toContain('border-t');
		await fireEvent.click(lootTrigger);
		expect(screen.getByText('Animal Muscle Oil')).not.toBeNull();
		expect(screen.getByRole('button', { name: 'Hide activity loot' })).not.toBeNull();
		expect(screen.queryByLabelText('Find an item')).toBeNull();
		const activityLoot = screen.getByTestId('activity-loot-list');
		expect(activityLoot.className).toContain('max-h-[24rem]');
		expect(activityLoot.className).toContain('overflow-y-auto');
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
		expect(screen.getByRole('button', { name: 'Show activity loot' })).not.toBeNull();
		expect(screen.queryByText('Animal Muscle Oil')).toBeNull();
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

		expect(screen.getByText('In loot')).not.toBeNull();
		const nets = screen.getAllByText('+5.00');
		expect(nets).toHaveLength(2);
		expect(nets.filter((value) => value.classList.contains('text-positive'))).toHaveLength(1);
	});

	it('suppresses an unverified historical reward instead of reading current quest settings', () => {
		render(HuntingSessionActivities, {
			props: {
				activities: [activity({ rewardStatus: 'unverified', key: 'quest:legacy' })],
				marketAvailable: false,
			},
		});

		expect(screen.getByText('Realised Net')).not.toBeNull();
		expect(screen.getByText('Realised Rate')).not.toBeNull();
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
