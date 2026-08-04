import { describe, expect, it, vi } from 'vitest';
import type { SessionDefinition } from '$lib/api';
import { ApiError } from '$lib/api';
import type { Quest, QuestFamily } from '$lib/types';
import { createDefinitionsModel, type DefinitionsModelDeps } from './definitionsModel.svelte';

function definition(overrides: Partial<SessionDefinition> = {}): SessionDefinition {
	return {
		id: '1',
		name: 'ARIS Dailies',
		adHocSegments: false,
		instanceCount: 0,
		createdAt: 1000,
		updatedAt: null,
		roster: [],
		...overrides,
	};
}

function family(id: string, name: string): QuestFamily {
	return {
		id,
		name,
		planet: 'ARIS',
		cooldownDurationHours: null,
		cooldownAnchor: 'pickup',
		cooldownExpiresAt: null,
		memberCount: 0,
		lastStartedAt: null,
		lastCompletedAt: null,
	};
}

function makeDeps(overrides: Partial<DefinitionsModelDeps> = {}): DefinitionsModelDeps {
	return {
		listDefinitions: vi.fn(async () => [definition()]),
		createDefinition: vi.fn(async () => definition({ id: '7' })),
		updateDefinition: vi.fn(async () => definition()),
		deleteDefinition: vi.fn(async () => {}),
		selectDefinition: vi.fn(async () => ({})),
		refreshTracking: vi.fn(async () => ({})),
		listFamilies: vi.fn(async () => [family('3', 'Daily Hunting 1')]),
		listQuests: vi.fn(async () => []),
		...overrides,
	};
}

describe('createDefinitionsModel', () => {
	it('loads the definition list and surfaces load failures', async () => {
		const deps = makeDeps();
		const model = createDefinitionsModel(deps);
		await model.loadDefinitions();
		expect(model.definitions).toHaveLength(1);
		expect(model.error).toBeNull();

		const failing = createDefinitionsModel(
			makeDeps({
				listDefinitions: vi.fn(async () => {
					throw new Error('boom');
				}),
			}),
		);
		await failing.loadDefinitions();
		expect(failing.error).toBe('boom');
	});

	it('shapes the selection write (numeric id, null withdraws) and refreshes', async () => {
		const deps = makeDeps();
		const model = createDefinitionsModel(deps);
		await model.select('4');
		expect(deps.selectDefinition).toHaveBeenCalledWith(4);
		await model.select(null);
		expect(deps.selectDefinition).toHaveBeenCalledWith(null);
		expect(deps.refreshTracking).toHaveBeenCalledTimes(2);
	});

	it('drafts a roster with dedupe, ordering moves, and removal', () => {
		const model = createDefinitionsModel(makeDeps());
		model.openCreate();
		model.addFamily(family('3', 'Daily Hunting 1'));
		model.addFamily(family('3', 'Daily Hunting 1'));
		model.addQuest({ id: '9', name: 'The Ultimate Threat' } as Quest);
		model.addSegment('  Warm-up  ');
		model.addSegment('   ');
		expect(model.roster.map((entry) => entry.displayName)).toEqual([
			'Daily Hunting 1',
			'The Ultimate Threat',
			'Warm-up',
		]);

		model.moveEntry(2, -1);
		expect(model.roster.map((entry) => entry.displayName)).toEqual([
			'Daily Hunting 1',
			'Warm-up',
			'The Ultimate Threat',
		]);
		model.moveEntry(0, -1);
		expect(model.roster[0].displayName).toBe('Daily Hunting 1');

		model.removeEntry(1);
		expect(model.roster.map((entry) => entry.displayName)).toEqual([
			'Daily Hunting 1',
			'The Ultimate Threat',
		]);
	});

	it('saves a create, selects the new definition, and closes', async () => {
		const deps = makeDeps();
		const model = createDefinitionsModel(deps);
		model.openCreate();
		model.name = '  General Hunting  ';
		model.addSegment('Grind');
		expect(await model.save()).toBe(true);
		expect(deps.createDefinition).toHaveBeenCalledWith({
			name: 'General Hunting',
			ad_hoc_segments: false,
			roster: [{ kind: 'segment', ref_id: null, label: 'Grind' }],
		});
		expect(deps.selectDefinition).toHaveBeenCalledWith(7);
		expect(model.mode).toBe('closed');
	});

	it('tolerates the fixed-while-active conflict on the post-create selection', async () => {
		const deps = makeDeps({
			selectDefinition: vi.fn(async () => {
				throw new ApiError('conflict', 'fixed for the active session');
			}),
		});
		const model = createDefinitionsModel(deps);
		model.openCreate();
		model.name = 'General Hunting';
		expect(await model.save()).toBe(true);
		expect(model.authoringError).toBeNull();
	});

	it('refuses a blank name client-side', async () => {
		const model = createDefinitionsModel(makeDeps());
		model.openCreate();
		model.name = '   ';
		expect(await model.save()).toBe(false);
		expect(model.authoringError).toBe('A session needs a name');
	});

	it('drops a dead reference from the saved roster and updates in place', async () => {
		const deps = makeDeps();
		const model = createDefinitionsModel(deps);
		model.openEdit(
			definition({
				id: '2',
				roster: [
					{ id: '1', kind: 'quest_family', refId: '3', label: null, displayName: null },
					{ id: '2', kind: 'segment', refId: null, label: 'Grind', displayName: 'Grind' },
				],
			}),
		);
		expect(model.roster[0].missing).toBe(true);
		expect(await model.save()).toBe(true);
		expect(deps.updateDefinition).toHaveBeenCalledWith('2', {
			name: 'ARIS Dailies',
			ad_hoc_segments: false,
			roster: [{ kind: 'segment', ref_id: null, label: 'Grind' }],
		});
	});

	it('deletes only on the armed second step', async () => {
		const deps = makeDeps();
		const model = createDefinitionsModel(deps);
		model.openEdit(definition({ id: '2' }));
		expect(await model.deleteEditing()).toBe(false);
		expect(deps.deleteDefinition).not.toHaveBeenCalled();
		expect(model.deleteArmed).toBe(true);
		expect(await model.deleteEditing()).toBe(true);
		expect(deps.deleteDefinition).toHaveBeenCalledWith('2');
		expect(model.mode).toBe('closed');
	});
});
