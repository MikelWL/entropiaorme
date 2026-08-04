// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { SessionDefinition } from '$lib/api';
import DefinitionPicker from './DefinitionPicker.svelte';
import { createDefinitionsModel, type DefinitionsModelDeps } from './definitionsModel.svelte';

function definition(id: string, name: string): SessionDefinition {
	return {
		id,
		name,
		adHocSegments: false,
		isProtected: false,
		instanceCount: 0,
		createdAt: 1000,
		updatedAt: null,
		roster: [],
	};
}

function makeDeps(definitions: SessionDefinition[]): DefinitionsModelDeps {
	return {
		listDefinitions: vi.fn(async () => definitions),
		createDefinition: vi.fn(async () => definitions[0]),
		updateDefinition: vi.fn(async () => definitions[0]),
		deleteDefinition: vi.fn(async () => {}),
		selectDefinition: vi.fn(async () => ({})),
		refreshTracking: vi.fn(async () => ({})),
		listFamilies: vi.fn(async () => []),
		listQuests: vi.fn(async () => []),
	};
}

async function mount(definitions: SessionDefinition[], selectedId: string | null) {
	const deps = makeDeps(definitions);
	const model = createDefinitionsModel(deps);
	await model.loadDefinitions();
	const onOpenAuthoring = vi.fn();
	render(DefinitionPicker, { props: { model, selectedId, onOpenAuthoring } });
	return { deps, model, onOpenAuthoring };
}

describe('DefinitionPicker', () => {
	it('titles the island with the selected session and offers the switch', async () => {
		await mount([definition('1', 'ARIS Dailies'), definition('2', 'General Hunting')], '1');

		expect(screen.getByText('Session:')).toBeTruthy();
		const trigger = screen.getByLabelText('Switch session (currently ARIS Dailies)');
		expect(trigger.getAttribute('aria-haspopup')).toBe('menu');
		expect(screen.queryByRole('menu')).toBeNull();

		await fireEvent.click(trigger);
		expect(screen.getByRole('menu')).toBeTruthy();
		expect(screen.getByText('General Hunting')).toBeTruthy();
	});

	it('writes the selection when another session is picked', async () => {
		const { deps } = await mount(
			[definition('1', 'ARIS Dailies'), definition('2', 'General Hunting')],
			'1',
		);

		await fireEvent.click(screen.getByLabelText('Switch session (currently ARIS Dailies)'));
		await fireEvent.click(screen.getByText('General Hunting'));

		expect(deps.selectDefinition).toHaveBeenCalledWith(2);
	});

	it('offers only the create control until a session is authored', async () => {
		const { onOpenAuthoring } = await mount([], null);

		expect(screen.getByText('Session:')).toBeTruthy();
		expect(screen.queryByTitle('Switch session')).toBeNull();

		await fireEvent.click(screen.getByTitle('Create a session'));
		expect(onOpenAuthoring).toHaveBeenCalledWith(null);
	});

	it('edits a session in place from its row', async () => {
		const { onOpenAuthoring } = await mount([definition('1', 'ARIS Dailies')], '1');

		await fireEvent.click(screen.getByLabelText('Switch session (currently ARIS Dailies)'));
		await fireEvent.click(screen.getByLabelText('Edit ARIS Dailies'));

		expect(onOpenAuthoring).toHaveBeenCalledWith('1');
	});

	// The selection is a configuration facet, so an unselected picker is a
	// real state (a session cleared, or one deleted since): it invites the
	// choice rather than rendering a stale name.
	it('invites a choice when nothing is selected', async () => {
		await mount([definition('1', 'ARIS Dailies')], null);

		expect(screen.getByLabelText('Choose a session')).toBeTruthy();
		await fireEvent.click(screen.getByLabelText('Choose a session'));
		expect(screen.queryByText('None')).toBeNull();
	});
});
