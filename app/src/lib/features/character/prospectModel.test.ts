import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
	CharacterProspectOptions,
	ProspectOption,
	ProspectResult,
} from '$lib/api/commands.gen';
import { createOptimizerModel, type PageErrorSlot } from './optimizerModel.svelte';
import {
	createProspectModel,
	formatGain,
	formatProfLevel,
	formatProspectHours,
	gainColorClass,
} from './prospectModel.svelte';

vi.mock('$lib/api', () => ({
	getProfessionOptimizer: vi.fn(),
	getHpOptimizer: vi.fn(),
	getProfessionPathOptimizer: vi.fn(),
	getCharacterProspect: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function option(value: string): ProspectOption {
	return { value, label: value, sessions: 3, kills: 120, hours: 2.5, cycledPed: 480 };
}

function prospectOptions(): CharacterProspectOptions {
	return {
		tags: [option('tag-a')],
		mobs: [option('mob-a'), option('mob-b')],
		weapons: [option('weapon-a')],
	};
}

function prospectResult(overrides: Partial<ProspectResult> = {}): ProspectResult {
	return {
		rows: [],
		warnings: [],
		profession: 'Laser Sniper (Hit)',
		sliceType: 'global',
		sliceValue: null,
		markupUplift: 0,
		currentLevel: 40,
		targetLevel: 50,
		projectedCycledPed: 12000,
		projectedHours: 60,
		expectedLootTt: 11400,
		expectedNetTtBurn: 600,
		speculativeLootTt: null,
		speculativeNetTtBurn: null,
		sample: { sessions: 3, hours: 2.5, cycledPed: 480, returnRate: 0.95, pesPerPed: 0.001 },
		...overrides,
	} as ProspectResult;
}

function makeModel() {
	const errors: PageErrorSlot = { error: null };
	const optimizer = createOptimizerModel(errors);
	const model = createProspectModel(optimizer, errors);
	return { model, optimizer, errors };
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('currentOptions', () => {
	it('follows the slice type and is empty for global', () => {
		const { model } = makeModel();
		model.options = prospectOptions();
		expect(model.currentOptions).toEqual([]);
		model.sliceType = 'tag';
		expect(model.currentOptions.map((o) => o.value)).toEqual(['tag-a']);
		model.sliceType = 'mob';
		expect(model.currentOptions.map((o) => o.value)).toEqual(['mob-a', 'mob-b']);
		model.sliceType = 'weapon';
		expect(model.currentOptions.map((o) => o.value)).toEqual(['weapon-a']);
	});
});

describe('loadProspect validation', () => {
	it('does nothing without a selected profession', async () => {
		const { model } = makeModel();
		model.targetInput = '50';
		await model.loadProspect();
		expect(mocked.getCharacterProspect).not.toHaveBeenCalled();
	});

	it('does nothing when the target is missing, non-numeric, zero or negative', async () => {
		const { model, optimizer } = makeModel();
		optimizer.selectedProfession = 'Laser Sniper (Hit)';
		for (const target of ['', 'abc', '0', '-5']) {
			model.targetInput = target;
			await model.loadProspect();
		}
		expect(mocked.getCharacterProspect).not.toHaveBeenCalled();
	});

	it('requires a slice value for a non-global slice', async () => {
		const { model, optimizer } = makeModel();
		optimizer.selectedProfession = 'Laser Sniper (Hit)';
		model.targetInput = '50';
		model.sliceType = 'mob';
		model.sliceValue = '';
		await model.loadProspect();
		expect(mocked.getCharacterProspect).not.toHaveBeenCalled();
	});

	it('clears a stale error on entry even when validation stops the load', async () => {
		const { model, errors } = makeModel();
		errors.error = 'stale failure';
		await model.loadProspect();
		expect(errors.error).toBeNull();
	});
});

describe('loadProspect', () => {
	it('sends a global query with no slice value and a zeroed markup for empty input', async () => {
		const { model, optimizer } = makeModel();
		mocked.getCharacterProspect.mockResolvedValue(prospectResult());
		optimizer.selectedProfession = 'Laser Sniper (Hit)';
		model.targetInput = '50';
		await model.loadProspect();

		expect(mocked.getCharacterProspect).toHaveBeenCalledWith({
			profession: 'Laser Sniper (Hit)',
			targetLevel: 50,
			sliceType: 'global',
			sliceValue: undefined,
			markupUplift: 0,
		});
		expect(model.result?.targetLevel).toBe(50);
		expect(model.loading).toBe(false);
	});

	it('sends the slice value for a non-global slice and scales the markup to a fraction', async () => {
		const { model, optimizer } = makeModel();
		mocked.getCharacterProspect.mockResolvedValue(prospectResult());
		optimizer.selectedProfession = 'Laser Sniper (Hit)';
		model.targetInput = '50';
		model.sliceType = 'mob';
		model.sliceValue = 'mob-a';
		model.markupInput = '5';
		await model.loadProspect();

		expect(mocked.getCharacterProspect).toHaveBeenCalledWith(
			expect.objectContaining({ sliceType: 'mob', sliceValue: 'mob-a', markupUplift: 0.05 }),
		);
	});

	it('clamps a negative markup to zero and treats a non-numeric one as zero', async () => {
		const { model, optimizer } = makeModel();
		mocked.getCharacterProspect.mockResolvedValue(prospectResult());
		optimizer.selectedProfession = 'Laser Sniper (Hit)';
		model.targetInput = '50';

		model.markupInput = '-3';
		await model.loadProspect();
		expect(mocked.getCharacterProspect).toHaveBeenLastCalledWith(
			expect.objectContaining({ markupUplift: 0 }),
		);

		model.markupInput = 'abc';
		await model.loadProspect();
		expect(mocked.getCharacterProspect).toHaveBeenLastCalledWith(
			expect.objectContaining({ markupUplift: 0 }),
		);
	});

	it('surfaces a failure and lands the result on null', async () => {
		const { model, optimizer, errors } = makeModel();
		mocked.getCharacterProspect.mockResolvedValue(prospectResult());
		optimizer.selectedProfession = 'Laser Sniper (Hit)';
		model.targetInput = '50';
		await model.loadProspect();
		expect(model.result).not.toBeNull();

		mocked.getCharacterProspect.mockRejectedValue(new Error('no data'));
		await model.loadProspect();
		expect(model.result).toBeNull();
		expect(model.loading).toBe(false);
		expect(errors.error).toBe('no data');
	});
});

describe('formatProspectHours', () => {
	it('collapses zero and negative hours to 0h', () => {
		expect(formatProspectHours(0)).toBe('0h');
		expect(formatProspectHours(-2)).toBe('0h');
	});

	it('renders under an hour as rounded minutes', () => {
		expect(formatProspectHours(0.5)).toBe('30m');
		expect(formatProspectHours(0.99)).toBe('59m');
	});

	it('renders one decimal under ten hours and whole hours from ten up', () => {
		expect(formatProspectHours(1)).toBe('1.0h');
		expect(formatProspectHours(9.94)).toBe('9.9h');
		expect(formatProspectHours(10)).toBe('10h');
		expect(formatProspectHours(25.6)).toBe('26h');
	});
});

describe('formatGain', () => {
	it('renders a dash for null (no anchor on record)', () => {
		expect(formatGain(null)).toBe('\u2014');
	});

	it('collapses near-zero gains to an unsigned 0.00', () => {
		expect(formatGain(0)).toBe('0.00');
		expect(formatGain(0.0049)).toBe('0.00');
		expect(formatGain(-0.0049)).toBe('0.00');
	});

	it('prefixes positive gains with a plus and keeps the negative sign', () => {
		expect(formatGain(1.234)).toBe('+1.23');
		expect(formatGain(-2.5)).toBe('-2.50');
	});
});

describe('gainColorClass', () => {
	it('mutes null and near-zero gains', () => {
		expect(gainColorClass(null)).toBe('text-text-tertiary');
		expect(gainColorClass(0.0049)).toBe('text-text-tertiary');
	});

	it('colours positive gains as success and negative as warning', () => {
		expect(gainColorClass(0.01)).toBe('text-success');
		expect(gainColorClass(-0.01)).toBe('text-warning');
	});
});

describe('formatProfLevel', () => {
	it('renders a dash for null', () => {
		expect(formatProfLevel(null)).toBe('\u2014');
	});

	it('renders the floored level with the fractional part as a percentage', () => {
		expect(formatProfLevel(12.345)).toBe('12 (34.5%)');
		expect(formatProfLevel(50)).toBe('50 (0.0%)');
		expect(formatProfLevel(0.5)).toBe('0 (50.0%)');
	});
});
