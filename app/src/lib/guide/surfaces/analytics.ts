import { getDemoApi, guideState } from '../state.svelte';
import type { GuideSurface } from '../types';

/** Analytics-surface demoApi method names (declared here for documentation). */
type AnalyticsDemoApi = {
	setTab(tab: 'overview' | 'ledger' | 'hunting' | 'treecutting'): void;
};

/** Sub-API registered by LedgerTab.svelte on mount for guide-driven modal control. */
type LedgerDemoApi = {
	openAddEntryModal(): void;
	closeAddEntryModal(): void;
	openInventorySellModal(itemName: string, prefilledPrice?: number): void;
	closeInventorySellModal(): void;
	injectDemoSaleEntry(itemName: string, gain: number): void;
	clearDemoSaleEntry(): void;
};

function analyticsApi(): Partial<AnalyticsDemoApi> {
	return getDemoApi('analytics') as Partial<AnalyticsDemoApi>;
}

function ledgerApi(): Partial<LedgerDemoApi> {
	return getDemoApi('analytics-ledger') as Partial<LedgerDemoApi>;
}

/** Sub-API registered by HuntingTab.svelte on mount for guide-driven view
 * switching. */
type HuntingDemoApi = {
	setView(view: 'sessions' | 'targets' | 'market' | 'history'): void;
};

function huntingApi(): Partial<HuntingDemoApi> {
	return getDemoApi('analytics-hunting') as Partial<HuntingDemoApi>;
}

/** Sleep in 200ms chunks so loop iterations can break promptly on Next / Back / Close. */
async function abortableWait(ms: number, stillActive: () => boolean): Promise<boolean> {
	const end = Date.now() + ms;
	while (Date.now() < end) {
		await new Promise((r) => setTimeout(r, Math.min(200, end - Date.now())));
		if (!stillActive()) return false;
	}
	return true;
}

export const analyticsSurface: GuideSurface = {
	id: 'analytics',
	title: 'Analytics',
	beforeStart(demoApi) {
		const api = demoApi as Partial<AnalyticsDemoApi>;
		api.setTab?.('overview');
	},
	steps: [
		{
			id: 'overview-intro',
			anchor: () =>
				document.querySelector<HTMLElement>('[data-guide-anchor="analytics-overview-area"]'),
			placement: 'top-right',
			prose: {
				title: 'Analytics',
				body: 'The Analytics tab combines your tracked hunts and out-of-gameplay data into one unified view.',
				note: 'Note: Guide uses demo data.',
			},
		},
		{
			id: 'ledger-intro',
			anchor: () =>
				document.querySelector<HTMLElement>('[data-guide-anchor="analytics-ledger-area"]'),
			placement: 'top-right',
			prose: {
				title: 'Ledger',
				body: [
					{
						kind: 'p',
						text: 'The Ledger records your out-of-gameplay activity. There are two surfaces:',
					},
					{
						kind: 'ul',
						items: ['The main ledger.', 'The inventory.'],
					},
				],
			},
			async play({ demoApi, wait }) {
				const api = demoApi as Partial<AnalyticsDemoApi>;
				api.setTab?.('ledger');
				await wait(500);
			},
			resetDemo() {
				analyticsApi().setTab?.('overview');
			},
		},
		{
			id: 'ledger-add-entry',
			// Cutout target is dynamic: highlight the dialog while it's open, the main
			// ledger area (strip + table) otherwise. The 120ms anchor poll + 350ms CSS
			// path transition give a smooth shift each loop iteration.
			anchor: () => {
				const dialog = document.querySelector<HTMLElement>(
					'[role="dialog"][aria-label="Add Entry"]',
				);
				if (dialog && dialog.offsetParent !== null) return dialog;
				return document.querySelector<HTMLElement>(
					'[data-guide-anchor="analytics-ledger-main-area"]',
				);
			},
			placement: 'bottom-left',
			prose: {
				title: 'Add entry',
				body: 'Add gains and expenses to your ledger. This could include markup gained from sales, costs of travelling between planets, etc.',
			},
			async play({ cursor, demoApi, wait }) {
				const stepIdx = guideState.currentStepIndex;
				const stillActive = () => guideState.isActive && guideState.currentStepIndex === stepIdx;

				const aapi = demoApi as Partial<AnalyticsDemoApi>;
				aapi.setTab?.('ledger');
				await wait(500);
				if (!stillActive()) return;

				// LedgerTab registers its sub-API on mount; poll briefly for it.
				for (let i = 0; i < 40; i++) {
					if (ledgerApi().openAddEntryModal) break;
					await wait(50);
					if (!stillActive()) return;
				}

				while (stillActive()) {
					const addBtn = document.querySelector<HTMLElement>(
						'[data-guide-anchor="ledger-add-entry-btn"]',
					);
					if (!addBtn) {
						if (!(await abortableWait(200, stillActive))) return;
						continue;
					}
					// Reset cursor to a neutral stage-left start each iteration so the slide is visible.
					const btnRect = addBtn.getBoundingClientRect();
					const startX = Math.max(40, btnRect.left - 320);
					const startY = btnRect.top + btnRect.height / 2;
					const startRect = new DOMRect(startX, startY, 0, 0);
					await cursor.moveTo(startRect, { duration: 0 });
					cursor.show();
					if (!stillActive()) break;
					await cursor.moveTo(addBtn, { duration: 900, from: { x: startX, y: startY } });
					if (!stillActive()) break;
					await cursor.clickRipple();
					cursor.hide();
					if (!stillActive()) break;
					ledgerApi().openAddEntryModal?.();
					if (!(await abortableWait(5000, stillActive))) break;
					ledgerApi().closeAddEntryModal?.();
					if (!(await abortableWait(700, stillActive))) break;
				}

				// Cleanup: dialog stays closed and cursor stays hidden when stepping away.
				ledgerApi().closeAddEntryModal?.();
				cursor.hide();
			},
			resetDemo() {
				ledgerApi().closeAddEntryModal?.();
			},
		},
		{
			id: 'inventory-sell-flow',
			// Presence-driven dynamic anchor: dialog while the Sell modal is open,
			// the synthetic new-entry row while it exists, inventory area otherwise.
			// The 120ms anchor poll + 350ms CSS path transition smooth-shift the
			// cutout between the three phases each loop iteration.
			anchor: () => {
				const dialog = document.querySelector<HTMLElement>('[role="dialog"][aria-label^="Sell "]');
				if (dialog && dialog.offsetParent !== null) return dialog;
				const entryRow = document.querySelector<HTMLElement>(
					'[data-guide-anchor="ledger-entry-row"][data-entry-id="demo-inventory-sale"]',
				);
				if (entryRow && entryRow.offsetParent !== null) return entryRow;
				return document.querySelector<HTMLElement>(
					'[data-guide-anchor="analytics-ledger-inventory-area"]',
				);
			},
			placement: 'bottom-left',
			prose: {
				title: 'Inventory',
				body: 'Manage inventory items by initial purchase price and sale price. The difference is added to your ledger.',
			},
			async play({ cursor, demoApi, wait }) {
				const stepIdx = guideState.currentStepIndex;
				const stillActive = () => guideState.isActive && guideState.currentStepIndex === stepIdx;

				const aapi = demoApi as Partial<AnalyticsDemoApi>;
				aapi.setTab?.('ledger');
				await wait(500);
				if (!stillActive()) return;

				// LedgerTab registers its sub-API on mount; poll briefly for it.
				for (let i = 0; i < 40; i++) {
					if (ledgerApi().openInventorySellModal) break;
					await wait(50);
					if (!stillActive()) return;
				}

				const ITEM_NAME = 'Hedoc Mayhem, Adjusted';
				const SALE_PRICE = 1360; // cost basis (720 TT + 540 markup) + 100 PED gain
				const GAIN = 100;

				while (stillActive()) {
					// === Phase A: scroll inventory into view + cursor → Sell button ===
					const inventoryArea = document.querySelector<HTMLElement>(
						'[data-guide-anchor="analytics-ledger-inventory-area"]',
					);
					if (inventoryArea) {
						inventoryArea.scrollIntoView({ behavior: 'smooth', block: 'start' });
						if (!(await abortableWait(500, stillActive))) break;
					}

					const sellBtn = document.querySelector<HTMLElement>(
						`[data-guide-anchor="inventory-sell-btn"][data-item-name="${ITEM_NAME}"]`,
					);
					if (!sellBtn) {
						if (!(await abortableWait(200, stillActive))) return;
						continue;
					}
					const btnRect = sellBtn.getBoundingClientRect();
					const startX = Math.max(40, btnRect.left - 320);
					const startY = btnRect.top + btnRect.height / 2;
					const startRect = new DOMRect(startX, startY, 0, 0);
					await cursor.moveTo(startRect, { duration: 0 });
					cursor.show();
					if (!stillActive()) break;
					await cursor.moveTo(sellBtn, { duration: 900, from: { x: startX, y: startY } });
					if (!stillActive()) break;
					await cursor.clickRipple();
					cursor.hide();
					if (!stillActive()) break;

					// === Phase B: open Sell modal pre-filled, dwell, cursor → Confirm Sale ===
					ledgerApi().openInventorySellModal?.(ITEM_NAME, SALE_PRICE);
					if (!(await abortableWait(1200, stillActive))) break;

					const dialog = document.querySelector<HTMLElement>(
						'[role="dialog"][aria-label^="Sell "]',
					);
					if (!dialog) {
						ledgerApi().closeInventorySellModal?.();
						if (!(await abortableWait(200, stillActive))) return;
						continue;
					}
					let confirmBtn: HTMLElement | null = null;
					for (const btn of Array.from(dialog.querySelectorAll<HTMLElement>('button'))) {
						if (btn.textContent?.trim() === 'Confirm Sale') {
							confirmBtn = btn;
							break;
						}
					}
					if (!confirmBtn) {
						ledgerApi().closeInventorySellModal?.();
						if (!(await abortableWait(200, stillActive))) return;
						continue;
					}
					const confirmRect = confirmBtn.getBoundingClientRect();
					const confirmStartX = Math.max(40, confirmRect.left - 320);
					const confirmStartY = confirmRect.top + confirmRect.height / 2;
					const confirmStartRect = new DOMRect(confirmStartX, confirmStartY, 0, 0);
					await cursor.moveTo(confirmStartRect, { duration: 0 });
					cursor.show();
					if (!stillActive()) break;
					await cursor.moveTo(confirmBtn, {
						duration: 900,
						from: { x: confirmStartX, y: confirmStartY },
					});
					if (!stillActive()) break;
					await cursor.clickRipple();
					cursor.hide();
					if (!stillActive()) break;

					// === Phase C: close modal + inject synthetic ledger entry, dwell ===
					ledgerApi().closeInventorySellModal?.();
					ledgerApi().injectDemoSaleEntry?.(ITEM_NAME, GAIN);
					if (!(await abortableWait(300, stillActive))) break;

					const entryRow = document.querySelector<HTMLElement>(
						'[data-guide-anchor="ledger-entry-row"][data-entry-id="demo-inventory-sale"]',
					);
					if (entryRow) {
						entryRow.scrollIntoView({ behavior: 'smooth', block: 'center' });
					}
					if (!(await abortableWait(3000, stillActive))) break;

					// === Phase R: clear synthetic entry, gap, loop ===
					ledgerApi().clearDemoSaleEntry?.();
					if (!(await abortableWait(700, stillActive))) break;
				}

				// Cleanup: modal closed, entry cleared, cursor hidden on step exit.
				ledgerApi().closeInventorySellModal?.();
				ledgerApi().clearDemoSaleEntry?.();
				cursor.hide();
			},
			resetDemo() {
				ledgerApi().closeInventorySellModal?.();
				ledgerApi().clearDemoSaleEntry?.();
			},
		},
		{
			id: 'hunting-intro',
			anchor: () =>
				document.querySelector<HTMLElement>('[data-guide-anchor="analytics-hunting-area"]'),
			placement: 'bottom-centre',
			prose: {
				title: 'Hunting',
				body: [
					{
						kind: 'p',
						text: 'The Hunting tab compares your play on two honest axes. Sessions ranks the routines you defined on the dashboard, with each routine opening onto the quests and segments played inside it. Targets ranks the species you hunted, with maturity as a drilldown.',
					},
					{
						kind: 'p',
						text: 'Market and History carry the same sale lifecycle as Tree Cutting: hold, list, confirm, and every action reversible from History. PES/100 remains the primary skilling comparison across both axes.',
					},
					{
						kind: 'p',
						text: 'Figures here are direct hunting cost (weapon and enhancer at kill grain); heal and armour stay with the full session economics on the Dashboard and Overview.',
					},
				],
				note: [
					{ text: 'See ' },
					{ text: 'What is PES?', href: 'https://entropiaorme.com/articles/what-is-pes' },
					{ text: '.' },
				],
			},
			async play({ demoApi, wait }) {
				const stepIdx = guideState.currentStepIndex;
				const stillActive = () => guideState.isActive && guideState.currentStepIndex === stepIdx;
				const api = demoApi as Partial<AnalyticsDemoApi>;
				api.setTab?.('hunting');
				await wait(600);
				// Walk the two comparison axes the prose describes, then come
				// back to rest on Sessions.
				if (!(await abortableWait(900, stillActive))) return;
				huntingApi().setView?.('targets');
				if (!(await abortableWait(1600, stillActive))) return;
				huntingApi().setView?.('sessions');
			},
			resetDemo() {
				huntingApi().setView?.('sessions');
				analyticsApi().setTab?.('ledger');
			},
		},
	],
};
