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
	setView(view: 'overall' | 'market' | 'history'): void;
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
				body: 'The Ledger is the accounting record for gains and expenses outside tracked gameplay. Assets and sales are managed from Inventory.',
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
			id: 'hunting-intro',
			anchor: () =>
				document.querySelector<HTMLElement>('[data-guide-anchor="analytics-hunting-area"]'),
			placement: 'bottom-centre',
			prose: {
				title: 'Hunting',
				body: [
					{
						kind: 'p',
						text: 'The Hunting tab applies the same economic frame as Tree Cutting at a larger scale. Overall is also the session picker: choosing a routine replaces the combined detail beneath the same headline figures. Inside a session, the Activity picker opens one declared quest, segment, or joint bundle directly without duplicating shared costs. Session and activity loot remain available beneath their respective details and unfold only when requested.',
					},
					{
						kind: 'p',
						text: 'Each row answers the same economic questions. Activity details also show whether a separately confirmed quest reward changed the outcome, while payouts already present in loot are never added twice.',
					},
					{
						kind: 'p',
						text: 'Stock, Market, and History share one progressive detail area inside Overall. Together they carry the same sale lifecycle as Tree Cutting: hold, list, confirm, and reverse an action from History. A confirmed sale is attributed back to the defined session that produced its stock.',
					},
				],
			},
			async play({ demoApi, wait }) {
				const stepIdx = guideState.currentStepIndex;
				const stillActive = () => guideState.isActive && guideState.currentStepIndex === stepIdx;
				const api = demoApi as Partial<AnalyticsDemoApi>;
				api.setTab?.('hunting');
				await wait(600);
				// Briefly show the sale surface, then return to Overall.
				if (!(await abortableWait(900, stillActive))) return;
				huntingApi().setView?.('market');
				if (!(await abortableWait(1600, stillActive))) return;
				huntingApi().setView?.('overall');
			},
			resetDemo() {
				huntingApi().setView?.('overall');
				analyticsApi().setTab?.('ledger');
			},
		},
	],
};
