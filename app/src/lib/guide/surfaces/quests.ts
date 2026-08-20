import { getDemoApi } from '../state.svelte';
import type { GuideSurface } from '../types';

/** Quests-surface demoApi method names (declared here for documentation). */
type QuestsDemoApi = {
	setView(view: 'quests' | 'families' | 'review' | 'analytics'): void;
	openNewQuestModal(): void;
	closeNewQuestModal(): void;
	closeFamilyModal(): void;
};

function questsApi(): Partial<QuestsDemoApi> {
	return getDemoApi('quests') as Partial<QuestsDemoApi>;
}

export const questsSurface: GuideSurface = {
	id: 'quests',
	title: 'Quests',
	beforeStart(demoApi) {
		const api = demoApi as Partial<QuestsDemoApi>;
		api.setView?.('quests');
		api.closeNewQuestModal?.();
		api.closeFamilyModal?.();
	},
	steps: [
		{
			id: 'narrative-intro',
			prose: {
				title: 'Quests',
				body: [
					{ kind: 'p', text: 'The Quests tab helps with:' },
					{
						kind: 'ul',
						items: [
							'Cooldown timers.',
							'Automatically detecting quest starts and completions, with the configured extra reward kept separate from ordinary activity loot.',
							'Analysing the observed rewards and play attributed to each quest.',
						],
					},
				],
				note: 'Note: Guide uses demo data.',
			},
		},
		{
			id: 'new-quest-form',
			anchor: () =>
				document.querySelector('[role="dialog"][aria-label="New Quest"]') as HTMLElement | null,
			prose: {
				title: 'Creating a quest',
				body: [
					{ kind: 'p', text: 'When creating a quest:' },
					{
						kind: 'ul',
						items: [
							'The name must match the in-game quest name. chat.log is read to automatically detect when a quest has been started/completed.',
							'Choose what proves completion, then define whether the extra reward is fixed PES, specific items, an isolated hand-in payout, or nothing separate.',
							'Additional details are for your convenience.',
						],
					},
				],
			},
			async play({ demoApi, wait }) {
				const api = demoApi as Partial<QuestsDemoApi>;
				api.openNewQuestModal?.();
				await wait(500);
			},
			resetDemo() {
				questsApi().closeNewQuestModal?.();
			},
		},
		{
			id: 'families-overview',
			anchor: () =>
				document.querySelector<HTMLElement>('[data-guide-anchor="quests-families-view"]'),
			prose: {
				title: 'Families',
				body: 'A family groups the rotating variants of one repeatable slot (a daily whose giver hands out a different variant each day) so they share one cooldown: doing any variant gates every sibling. The timer can run from pickup (collecting at the giver) or completion, and a newly received variant attaches to its family automatically.',
			},
			async play({ demoApi, wait }) {
				const api = demoApi as Partial<QuestsDemoApi>;
				api.closeNewQuestModal?.();
				api.setView?.('families');
				await wait(500);
			},
			resetDemo() {
				questsApi().setView?.('quests');
			},
		},
		{
			id: 'analytics-tip',
			prose: {
				title: 'Quest analytics',
				body: 'Tip: Add quests to a session definition, then declare the quest you are playing from the Activities control. The same authored roster appears on the dashboard, and those exact stretches drive quest attribution.',
			},
			async play({ demoApi, wait }) {
				const api = demoApi as Partial<QuestsDemoApi>;
				api.setView?.('analytics');
				await wait(500);
			},
			resetDemo() {
				questsApi().setView?.('quests');
			},
		},
	],
};
