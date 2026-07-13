/**
 * Activity recommender view model: the ranking target, the ranked
 * arbitrage candidates with their projected gain series, and the
 * selected candidate the chart renders. Presentation lives in the
 * feature components; they compose over this state. Failures land in
 * the page-level error slot the character model shares across the
 * surface.
 */

import { getActivityRecommender } from '$lib/api';
import type { ActivityRecommenderResult, RecommenderActivity } from '$lib/api/commands.gen';
import { describeError } from '$lib/view/errorState';
import { type CodexRankingTarget, targetProfessions } from './codexRankingTarget';
import type { PageErrorSlot } from './optimizerModel.svelte';

export function createRecommenderModel(errors: PageErrorSlot) {
	let target = $state<CodexRankingTarget>({ kind: 'none' });
	let result = $state<ActivityRecommenderResult | null>(null);
	let selectedActivity = $state('');
	let loading = $state(false);

	const candidates = $derived(result?.candidates ?? []);
	const selected = $derived<RecommenderActivity | null>(
		candidates.find((candidate) => candidate.activity === selectedActivity) ??
			candidates[0] ??
			null,
	);

	async function load(next: CodexRankingTarget) {
		target = next;
		result = null;
		selectedActivity = '';
		if (next.kind === 'none') return;
		errors.error = null;
		loading = true;
		try {
			const loaded = await getActivityRecommender(
				next.kind === 'hp'
					? { target: 'hp', professions: [] }
					: { target: 'profession', professions: targetProfessions(next) },
			);
			if (loaded.error) {
				errors.error = loaded.error;
				return;
			}
			result = loaded;
			selectedActivity = loaded.candidates[0]?.activity ?? '';
		} catch (e) {
			errors.error = describeError(e, 'Failed to load the activity recommender');
		} finally {
			loading = false;
		}
	}

	function select(activity: string) {
		selectedActivity = activity;
	}

	return {
		get target() {
			return target;
		},
		get result() {
			return result;
		},
		set result(value: ActivityRecommenderResult | null) {
			result = value;
		},
		get candidates() {
			return candidates;
		},
		get selected() {
			return selected;
		},
		get selectedActivity() {
			return selectedActivity;
		},
		get loading() {
			return loading;
		},

		load,
		select,
	};
}

export type RecommenderModel = ReturnType<typeof createRecommenderModel>;
