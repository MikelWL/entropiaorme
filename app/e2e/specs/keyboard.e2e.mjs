import { $, browser, expect } from '@wdio/globals';
import { Key } from 'webdriverio';
import { ensureDashboard } from '../helpers/onboarding.mjs';
import { DEV_URL } from '../wdio.conf.mjs';

// Keyboard-only smoke over the shared interactive primitives, in the real
// shell. The quests surface hosts both primitives under test: the form modal
// (Modal.svelte: focus trap, Escape dismiss, focus return) and the per-row
// three-dot menu (Menu.svelte: roving menuitem focus, arrow keys, Escape
// returning focus to the trigger). The backend is the real facade over an
// empty database, so the suite creates the one quest it needs through the
// app's own UI and removes it again; the after-hook purge makes the cleanup
// hold even when an assertion fails mid-test.

// Mirrors Modal.svelte's focusable selector so the trap assertion counts the
// same elements the trap itself cycles through.
const FOCUSABLE_SELECTOR =
	'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

const QUEST_NAME = 'Keyboard smoke quest';

function activeElementProbe() {
	return browser.execute(() => {
		const el = document.activeElement;
		return {
			tag: el ? el.tagName.toLowerCase() : null,
			role: el ? el.getAttribute('role') : null,
			ariaLabel: el ? el.getAttribute('aria-label') : null,
			text: el?.textContent ? el.textContent.trim() : '',
			inDialog: !!el?.closest('[role="dialog"]'),
			inMenu: !!el?.closest('[role="menu"]'),
		};
	});
}

// WebDriver has no first-class focus command; move focus with the DOM API and
// drive everything after that point through real key events.
async function focusElement(el) {
	await el.waitForExist({ timeout: 10000 });
	await browser.execute((node) => node.focus(), el);
}

describe('keyboard access (native Tauri shell)', () => {
	before(async () => {
		await ensureDashboard(browser, DEV_URL);
		await browser.url(`${DEV_URL}quests`);
		await expect($('h1')).toHaveText('Quests');
		// The list has loaded once the empty state (or a stray row) renders.
		await browser.waitUntil(async () => !(await $('main').getText()).includes('Loading quests'), {
			timeout: 15000,
			timeoutMsg: 'quests page never finished loading',
		});
	});

	// Belt-and-braces determinism: whatever happened above, leave the database
	// empty so the quests visual baselines (which pin the empty state) hold.
	after(async () => {
		const err = await browser.executeAsync(async (done) => {
			try {
				const invoke = window.__TAURI_INTERNALS__?.invoke;
				if (!invoke) {
					done('no IPC bridge');
					return;
				}
				const quests = await invoke('quests_list');
				for (const quest of quests) {
					await invoke('quest_delete', { quest_id: quest.id });
				}
				done(null);
			} catch (e) {
				done(String(e));
			}
		});
		if (err) throw new Error(`quest cleanup failed: ${err}`);
	});

	it('opens the quest modal with Enter, holds the focus trap, and returns focus on Escape', async () => {
		const newQuestBtn = await $('button*=+ Quest');
		await focusElement(newQuestBtn);
		await browser.keys(Key.Enter);

		const dialog = await $('[role="dialog"]');
		await dialog.waitForExist({ timeout: 10000 });
		// Modal moves focus into the panel on open.
		await browser.waitUntil(async () => (await activeElementProbe()).inDialog, {
			timeout: 5000,
			timeoutMsg: 'focus never moved into the dialog',
		});

		// Tab a full cycle (every focusable, plus two to prove the wrap): focus
		// must never escape the dialog while it is open.
		const focusableCount = await browser.execute((sel) => {
			const panel = document.querySelector('[role="dialog"]');
			return panel ? panel.querySelectorAll(sel).length : 0;
		}, FOCUSABLE_SELECTOR);
		expect(focusableCount).toBeGreaterThan(0);
		for (let i = 0; i < focusableCount + 2; i += 1) {
			await browser.keys(Key.Tab);
			const probe = await activeElementProbe();
			if (!probe.inDialog) {
				throw new Error(
					`focus escaped the dialog on Tab ${i + 1} of ${focusableCount + 2} (landed on <${probe.tag}> "${probe.text}")`,
				);
			}
		}

		// Escape closes and hands focus back to the trigger.
		await browser.keys(Key.Escape);
		await browser.waitUntil(async () => !(await $('[role="dialog"]').isExisting()), {
			timeout: 5000,
			timeoutMsg: 'dialog never closed on Escape',
		});
		await browser.waitUntil(async () => (await activeElementProbe()).text.includes('+ Quest'), {
			timeout: 5000,
			timeoutMsg: 'focus never returned to the + Quest trigger after Escape',
		});
	});

	it('drives the quest row menu with the keyboard alone', async () => {
		// Setup (pointer-driven; the keyboard assertions start at the menu): the
		// empty database has no rows, so create the quest whose row hosts the menu.
		const newQuestBtn = await $('button*=+ Quest');
		await newQuestBtn.click();
		const nameInput = await $('#q-name');
		await nameInput.waitForExist({ timeout: 10000 });
		await nameInput.setValue(QUEST_NAME);
		await $('button=Create').click();
		await browser.waitUntil(async () => !(await $('[role="dialog"]').isExisting()), {
			timeout: 10000,
			timeoutMsg: 'quest modal never closed after Create',
		});

		const trigger = await $('[aria-label="Quest actions"]');
		await trigger.waitForExist({ timeout: 10000 });

		// Enter on the focused trigger opens the menu and focus lands on the
		// first menuitem.
		await focusElement(trigger);
		await browser.keys(Key.Enter);
		await $('[role="menu"]').waitForExist({ timeout: 5000 });
		await expect(trigger).toHaveAttribute('aria-expanded', 'true');
		await browser.waitUntil(
			async () => {
				const probe = await activeElementProbe();
				return probe.role === 'menuitem' && probe.text === 'Edit';
			},
			{ timeout: 5000, timeoutMsg: 'focus never landed on the first menuitem' },
		);

		// Arrow keys rove across the menuitems and wrap.
		await browser.keys(Key.ArrowDown);
		expect((await activeElementProbe()).text).toBe('Delete');
		await browser.keys(Key.ArrowDown);
		expect((await activeElementProbe()).text).toBe('Edit');
		await browser.keys(Key.ArrowUp);
		expect((await activeElementProbe()).text).toBe('Delete');

		// Escape closes the menu and returns focus to the trigger.
		await browser.keys(Key.Escape);
		await browser.waitUntil(async () => !(await $('[role="menu"]').isExisting()), {
			timeout: 5000,
			timeoutMsg: 'menu never closed on Escape',
		});
		await browser.waitUntil(
			async () => (await activeElementProbe()).ariaLabel === 'Quest actions',
			{ timeout: 5000, timeoutMsg: 'focus never returned to the menu trigger after Escape' },
		);

		// Tear the quest down through the same menu (Enter activates Delete, the
		// panel swaps to its confirm pane), restoring the empty state the visual
		// specs pin.
		await browser.keys(Key.Enter);
		await $('[role="menu"]').waitForExist({ timeout: 5000 });
		await browser.keys(Key.ArrowDown);
		await browser.keys(Key.Enter);
		const confirmBtn = await $('button=Confirm');
		await confirmBtn.waitForClickable({ timeout: 5000 });
		await confirmBtn.click();
		await browser.waitUntil(async () => (await $('main').getText()).includes('No quests yet'), {
			timeout: 10000,
			timeoutMsg: 'quest list never returned to the empty state after delete',
		});
	});
});
