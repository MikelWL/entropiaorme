/**
 * Prospect view model: the slice/target/markup knobs over the profession
 * selection shared with the optimiser, the forecast load, and the pure
 * display formatters for the character surface. Presentation lives in the
 * feature components; they compose over this state.
 */

import { getCharacterProspect } from '$lib/api';
import type {
	CharacterProspectOptions,
	ProspectOption,
	ProspectResult,
	ProspectSliceType,
} from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';
import type { OptimizerModel, PageErrorSlot } from './optimizerModel.svelte';

export function createProspectModel(optimizer: OptimizerModel, errors: PageErrorSlot) {
	let options = $state<CharacterProspectOptions>({ tags: [], mobs: [], weapons: [] });
	let sliceType = $state<ProspectSliceType>('global');
	let sliceValue = $state('');
	let targetInput = $state('');
	let markupInput = $state('');
	let result = $state<ProspectResult | null>(null);
	let loading = $state(false);

	const currentOptions = $derived.by(() => {
		if (sliceType === 'tag') return options.tags;
		if (sliceType === 'mob') return options.mobs;
		if (sliceType === 'weapon') return options.weapons;
		return [] as ProspectOption[];
	});

	async function loadProspect() {
		errors.error = null;
		if (!optimizer.selectedProfession) return;
		const target = parseFloat(targetInput);
		if (Number.isNaN(target) || target <= 0) return;
		if (sliceType !== 'global' && !sliceValue) return;

		loading = true;
		result = null;
		try {
			result = await getCharacterProspect({
				profession: optimizer.selectedProfession,
				targetLevel: target,
				sliceType,
				sliceValue: sliceType === 'global' ? undefined : sliceValue,
				markupUplift: Math.max(0, (parseFloat(markupInput) || 0) / 100),
			});
		} catch (e) {
			result = null;
			errors.error = describeError(e, 'Failed to compute the prospect forecast');
		} finally {
			loading = false;
		}
	}

	return {
		get options() {
			return options;
		},
		set options(value: CharacterProspectOptions) {
			options = value;
		},
		get sliceType() {
			return sliceType;
		},
		set sliceType(value: ProspectSliceType) {
			sliceType = value;
		},
		get sliceValue() {
			return sliceValue;
		},
		set sliceValue(value: string) {
			sliceValue = value;
		},
		get targetInput() {
			return targetInput;
		},
		set targetInput(value: string) {
			targetInput = value;
		},
		get markupInput() {
			return markupInput;
		},
		set markupInput(value: string) {
			markupInput = value;
		},
		get result() {
			return result;
		},
		set result(value: ProspectResult | null) {
			result = value;
		},
		get loading() {
			return loading;
		},
		get currentOptions() {
			return currentOptions;
		},

		loadProspect,
	};
}

export type ProspectModel = ReturnType<typeof createProspectModel>;

// ── Display formatters ──

export function formatProspectHours(hours: number): string {
	if (hours <= 0) return '0h';
	if (hours < 1) return `${Math.round(hours * 60)}m`;
	if (hours < 10) return `${hours.toFixed(1)}h`;
	return `${hours.toFixed(0)}h`;
}

// Gain is shown with sign + 2dp; near-zero collapses to '0.00' so it doesn't
// flicker between '+0.00' and '-0.00'. Null = no anchor on record.
export function formatGain(gain: number | null): string {
	if (gain === null) return '\u2014';
	if (Math.abs(gain) < 0.005) return '0.00';
	return (gain > 0 ? '+' : '') + gain.toFixed(2);
}

export function gainColorClass(gain: number | null): string {
	if (gain === null || Math.abs(gain) < 0.005) return 'text-text-tertiary';
	return gain > 0 ? 'text-success' : 'text-warning';
}

export function formatProfLevel(level: number | null): string {
	if (level === null) return '\u2014';
	return `${Math.floor(level)} (${((level % 1) * 100).toFixed(1)}%)`;
}
