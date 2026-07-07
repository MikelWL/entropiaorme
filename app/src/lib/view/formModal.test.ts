import { describe, expect, it, vi } from 'vitest';
import { createFormModal } from './formModal.svelte';

interface QuestForm {
	name: string;
	reward: number | null;
}

interface Quest {
	id: string;
	name: string;
	reward: number | null;
}

const blank = (): QuestForm => ({ name: '', reward: null });
const toForm = (quest: Quest): QuestForm => ({ name: quest.name, reward: quest.reward });
const quest: Quest = { id: 'q1', name: 'Iron Challenge', reward: 12.5 };

function makeModal(save = vi.fn().mockResolvedValue(undefined)) {
	return { modal: createFormModal<QuestForm, Quest>({ blank, save }), save };
}

/** A manually resolvable save, for observing the mid-flight saving state. */
function deferredSave(): {
	save: ReturnType<typeof vi.fn>;
	resolve: () => void;
	reject: (err: unknown) => void;
} {
	let resolve!: () => void;
	let reject!: (err: unknown) => void;
	const promise = new Promise<void>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { save: vi.fn().mockReturnValue(promise), resolve, reject };
}

describe('opening', () => {
	it('starts closed with a blank form and nothing being edited', () => {
		const { modal } = makeModal();
		expect(modal.open).toBe(false);
		expect(modal.editing).toBeNull();
		expect(modal.form).toEqual(blank());
		expect(modal.saving).toBe(false);
		expect(modal.error).toBeNull();
	});

	it('openNew opens with a fresh blank form each time', () => {
		const { modal } = makeModal();
		modal.form = { name: 'leftover', reward: 3 };
		modal.openNew();
		expect(modal.open).toBe(true);
		expect(modal.editing).toBeNull();
		expect(modal.form).toEqual({ name: '', reward: null });
	});

	it('openEdit sets the editing entity and maps it through toForm', () => {
		const { modal } = makeModal();
		modal.openEdit(quest, toForm);
		expect(modal.open).toBe(true);
		expect(modal.editing).toEqual(quest);
		expect(modal.form).toEqual({ name: 'Iron Challenge', reward: 12.5 });
	});

	it('reopening clears the error from a previous failed submit', async () => {
		const { modal } = makeModal(vi.fn().mockRejectedValue(new Error('nope')));
		modal.openNew();
		await modal.submit();
		expect(modal.error).toBe('nope');

		modal.openNew();
		expect(modal.error).toBeNull();
		modal.openEdit(quest, toForm);
		expect(modal.error).toBeNull();
	});
});

describe('closing', () => {
	it('close hides the modal and forgets the edited entity', () => {
		const { modal } = makeModal();
		modal.openEdit(quest, toForm);
		modal.close();
		expect(modal.open).toBe(false);
		expect(modal.editing).toBeNull();
	});
});

describe('submit', () => {
	it('passes the current form and null editing to save on a create flow', async () => {
		const { modal, save } = makeModal();
		modal.openNew();
		modal.form = { name: 'New quest', reward: 5 };
		await modal.submit();
		expect(save).toHaveBeenCalledWith({ name: 'New quest', reward: 5 }, null);
	});

	it('passes the edited entity to save on an edit flow', async () => {
		const { modal, save } = makeModal();
		modal.openEdit(quest, toForm);
		await modal.submit();
		expect(save).toHaveBeenCalledWith({ name: 'Iron Challenge', reward: 12.5 }, quest);
	});

	it('closes and clears editing on success', async () => {
		const { modal } = makeModal();
		modal.openEdit(quest, toForm);
		await modal.submit();
		expect(modal.open).toBe(false);
		expect(modal.editing).toBeNull();
		expect(modal.error).toBeNull();
		expect(modal.saving).toBe(false);
	});

	it('exposes saving=true while the save is in flight', async () => {
		const d = deferredSave();
		const modal = createFormModal<QuestForm, Quest>({ blank, save: d.save });
		modal.openNew();
		const pending = modal.submit();
		expect(modal.saving).toBe(true);

		d.resolve();
		await pending;
		expect(modal.saving).toBe(false);
	});

	it('ignores a second submit while one is already saving', async () => {
		const d = deferredSave();
		const modal = createFormModal<QuestForm, Quest>({ blank, save: d.save });
		modal.openNew();
		const first = modal.submit();
		const second = modal.submit();
		d.resolve();
		await Promise.all([first, second]);
		expect(d.save).toHaveBeenCalledTimes(1);
	});

	it('stays open with the Error message captured on failure', async () => {
		const { modal } = makeModal(vi.fn().mockRejectedValue(new Error('quota exceeded')));
		modal.openEdit(quest, toForm);
		await modal.submit();
		expect(modal.open).toBe(true);
		expect(modal.editing).toEqual(quest);
		expect(modal.error).toBe('quota exceeded');
		expect(modal.saving).toBe(false);
	});

	it('stringifies a non-Error throw', async () => {
		const { modal } = makeModal(vi.fn().mockRejectedValue('plain failure'));
		modal.openNew();
		await modal.submit();
		expect(modal.error).toBe('plain failure');
	});

	it('clears the previous error at the start of a retry, then succeeds', async () => {
		const save = vi
			.fn()
			.mockRejectedValueOnce(new Error('first try'))
			.mockResolvedValueOnce(undefined);
		const { modal } = makeModal(save);
		modal.openNew();
		await modal.submit();
		expect(modal.error).toBe('first try');

		await modal.submit();
		expect(modal.error).toBeNull();
		expect(modal.open).toBe(false);
		expect(save).toHaveBeenCalledTimes(2);
	});

	it('allows a fresh submit after a failed one settles', async () => {
		const d = deferredSave();
		const modal = createFormModal<QuestForm, Quest>({ blank, save: d.save });
		modal.openNew();
		const first = modal.submit();
		d.reject(new Error('boom'));
		await first;
		expect(modal.saving).toBe(false);

		d.save.mockResolvedValue(undefined);
		await modal.submit();
		expect(modal.open).toBe(false);
		expect(d.save).toHaveBeenCalledTimes(2);
	});
});

describe('form binding', () => {
	it('accepts direct form replacement, as a bound modal form would produce', () => {
		const { modal } = makeModal();
		modal.openNew();
		modal.form = { name: 'Edited in place', reward: 1 };
		expect(modal.form).toEqual({ name: 'Edited in place', reward: 1 });
	});
});
