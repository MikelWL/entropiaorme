<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import type { UnlistenFn } from '@tauri-apps/api/event';
	import {
		OVERLAY_MENU_CLOSED_EVENT,
		OVERLAY_MENU_HIDE_EVENT,
		OVERLAY_MENU_INTERACT_EVENT,
		OVERLAY_MENU_READY_EVENT,
		OVERLAY_MENU_SELECT_EVENT,
		OVERLAY_MENU_SHOW_EVENT,
		type OverlayMenuSelection,
		type OverlayMenuState
	} from '$lib/overlayMenu';

	const MENU_MAX_HEIGHT = 220;

	const currentWindow = getCurrentWebviewWindow();

	let menuState = $state<OverlayMenuState | null>(null);
	let suppressBlurCloseUntil = 0;

	function getMobRowCount(state: Extract<OverlayMenuState, { kind: 'mob' }>): number {
		if (state.loading || state.error) return 1;
		if (state.mode === 'tag') return state.tagSuggestions.length > 0 ? state.tagSuggestions.length : 1;
		return state.mobSuggestions.length > 0 ? state.mobSuggestions.length : 1;
	}

	const popupHeight = $derived.by(() => {
		if (!menuState) return 1;
		const rows = menuState.kind === 'trifecta' ? menuState.options.length : getMobRowCount(menuState);
		return Math.min(MENU_MAX_HEIGHT, Math.max(44, rows * 34 + 12));
	});

	const popupWidth = $derived(menuState?.width ?? 1);

	async function requestClose() {
		if (!menuState) return;
		menuState = null;
		await currentWindow.hide().catch(() => {});
		await currentWindow.emitTo('overlay', OVERLAY_MENU_CLOSED_EVENT).catch(() => {});
	}

	async function handleSelection(selection: OverlayMenuSelection) {
		await currentWindow.emitTo('overlay', OVERLAY_MENU_SELECT_EVENT, selection).catch(() => {});
		await requestClose();
	}

	function signalInteraction() {
		if (!menuState) return;
		suppressBlurCloseUntil = Date.now() + 200;
		void currentWindow.emitTo('overlay', OVERLAY_MENU_INTERACT_EVENT).catch(() => {});
	}

	$effect(() => {
		let disposed = false;
		let unlistenShow: UnlistenFn | undefined;
		let unlistenHide: UnlistenFn | undefined;
		let unlistenFocus: UnlistenFn | undefined;

		const handleEscape = (event: KeyboardEvent) => {
			if (event.key !== 'Escape') return;
			event.preventDefault();
			void requestClose();
		};

		void (async () => {
			unlistenShow = await currentWindow.listen<OverlayMenuState>(OVERLAY_MENU_SHOW_EVENT, async (event) => {
				if (disposed) return;
				menuState = event.payload;
				suppressBlurCloseUntil = Date.now() + 200;
			});

			unlistenHide = await currentWindow.listen(OVERLAY_MENU_HIDE_EVENT, async () => {
				if (disposed) return;
				menuState = null;
				await currentWindow.hide().catch(() => {});
			});

			unlistenFocus = await currentWindow.onFocusChanged(({ payload: focused }) => {
				if (disposed || focused || !menuState) return;
				if (Date.now() < suppressBlurCloseUntil) return;
				void requestClose();
			});

			await currentWindow.emitTo('overlay', OVERLAY_MENU_READY_EVENT, { label: currentWindow.label }).catch(() => {});
		})();

		window.addEventListener('keydown', handleEscape);

		return () => {
			disposed = true;
			unlistenShow?.();
			unlistenHide?.();
			unlistenFocus?.();
			window.removeEventListener('keydown', handleEscape);
		};
	});
</script>

{#if menuState}
	<div
		class="overlay-menu-shell"
		role="menu"
		tabindex="-1"
		style:width={`${popupWidth}px`}
		style:height={`${popupHeight}px`}
		onpointerdown={signalInteraction}
		onwheel={signalInteraction}
	>
		{#if menuState.kind === 'trifecta'}
			<div class="menu-panel">
				{#each menuState.options as option}
					<button
						type="button"
						class="menu-option {option.active ? 'menu-option-active' : ''}"
						onclick={() => handleSelection({ kind: 'trifecta', presetId: option.id })}
					>
						<span class="menu-option-name">{option.name}</span>
						{#if option.active}
							<span class="menu-option-badge">Active</span>
						{/if}
					</button>
				{/each}
			</div>
		{:else}
			<div class="menu-panel">
				{#if menuState.loading}
					<div class="menu-empty">Searching...</div>
				{:else if menuState.error}
					<div class="menu-empty">{menuState.error}</div>
				{:else if menuState.mode === 'tag' && menuState.tagSuggestions.length === 0}
					<button
						type="button"
						class="menu-option"
						onclick={() => {
							if (menuState?.kind === 'mob') {
								handleSelection({ kind: 'tag', tag: menuState.query.trim() });
							}
						}}
					>
						<span class="menu-option-name">Press Enter to set "{menuState.query.trim()}"</span>
					</button>
				{:else if menuState.mode === 'manual' && menuState.mobSuggestions.length === 0}
					<div class="menu-empty">No matches</div>
				{:else if menuState.mode === 'tag'}
					{#each menuState.tagSuggestions as option}
						<button
							type="button"
							class="menu-option"
							onclick={() => handleSelection({ kind: 'tag', tag: option })}
						>
							<span class="menu-option-name">{option}</span>
						</button>
					{/each}
				{:else}
					{#each menuState.mobSuggestions as option}
						<button
							type="button"
							class="menu-option"
							onclick={() => handleSelection({ kind: 'mob', species: option.species, maturity: option.maturity })}
						>
							<span class="menu-option-name">{option.display}</span>
						</button>
					{/each}
				{/if}
			</div>
		{/if}
	</div>
{/if}

<style>
	.overlay-menu-shell {
		display: flex;
		align-items: stretch;
		justify-content: stretch;
	}

	.menu-panel {
		display: flex;
		flex-direction: column;
		width: 100%;
		max-height: 220px;
		overflow-y: auto;
		padding: 6px;
		border-radius: 8px;
		border: 1px solid rgba(255, 255, 255, 0.12);
		background: rgba(11, 15, 25, 0.96);
		backdrop-filter: blur(16px) saturate(150%);
		box-shadow:
			0 14px 30px rgba(0, 0, 0, 0.48),
			0 0 0 1px rgba(255, 255, 255, 0.03);
	}

	.menu-option {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		width: 100%;
		padding: 7px 8px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: rgba(255, 255, 255, 0.82);
		font-size: 12px;
		text-align: left;
		cursor: pointer;
		transition:
			background-color 140ms ease,
			color 140ms ease;
	}

	.menu-option:hover,
	.menu-option:focus-visible {
		background: rgba(255, 255, 255, 0.06);
		color: rgba(255, 255, 255, 0.94);
		outline: none;
	}

	.menu-option-active {
		background: rgba(56, 189, 248, 0.14);
		color: rgba(186, 230, 253, 0.98);
	}

	.menu-option-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.menu-option-badge {
		flex-shrink: 0;
		padding: 2px 6px;
		border-radius: 999px;
		background: rgba(56, 189, 248, 0.16);
		color: rgba(186, 230, 253, 0.96);
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	.menu-empty {
		padding: 8px 10px;
		color: rgba(255, 255, 255, 0.45);
		font-size: 11px;
	}
</style>
