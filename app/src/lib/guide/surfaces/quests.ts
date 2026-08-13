import { getDemoApi } from '../state.svelte';
import type { GuideSurface } from '../types';

/** Quests-surface demoApi method names (declared here for documentation). */
type QuestsDemoApi = {
	setView(view: 'quests' | 'families' | 'playlists' | 'review' | 'analytics'): void;
	openNewQuestModal(): void;
	closeNewQuestModal(): void;
	closePlaylistModal(): void;
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
		api.closePlaylistModal?.();
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
							'Analysing cost/reward of completing a quest or a quest playlist.',
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
							'Choose what proves completion, then define whether the extra reward is fixed PED, fixed PES, specific items, an isolated hand-in payout, or nothing separate.',
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
			id: 'playlists-overview',
			anchor: () =>
				document.querySelector<HTMLElement>('[data-guide-anchor="quests-playlists-view"]'),
			prose: {
				title: 'Playlists',
				body: 'By creating playlists, you can access them in the Quests dashboard widget to have them handy during gameplay, as well as analysing quest playlist rewards as one unit.',
			},
			async play({ demoApi, wait }) {
				const api = demoApi as Partial<QuestsDemoApi>;
				api.setView?.('playlists');
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
				body: 'Tip: While tracking, declare which quest your play is toward from the overlay quest picker. Analytics aggregate the sessions that recorded a declared stretch of a quest, so the picker is what turns gameplay into quest and playlist economics.',
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
