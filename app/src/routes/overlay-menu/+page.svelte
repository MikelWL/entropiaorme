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
	} from '$lib/windows/overlayMenu';

	const MENU_MAX_HEIGHT = 220;

	const currentWindow = getCurrentWebviewWindow();

	let menuState = $state<OverlayMenuState | null>(null);
	let suppressBlurCloseUntil = 0;

	/** Rows the panel will render, which drives the window height. Every
	 * kind falls back to one row: the loading, error, and empty states each
	 * occupy exactly one line. The focus picker counts its section heading
	 * as a row when presets follow the quests. */
	function getRowCount(state: OverlayMenuState): number {
		if (state.kind === 'trifecta') return Math.max(1, state.options.length);
		if (state.kind === 'focus') {
			const headings = state.presets.length > 0 ? 1 : 0;
			return Math.max(1, state.quests.length + state.presets.length + headings);
		}
		if (state.loading || state.error) return 1;
		if (state.kind === 'name') return Math.max(1, state.suggestions.length);
		return Math.max(1, state.mobSuggestions.length);
	}

	const popupHeight = $derived.by(() => {
		if (!menuState) return 1;
		return Math.min(MENU_MAX_HEIGHT, Math.max(44, getRowCount(menuState) * 34 + 12));
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

	/** A focus quest toggle keeps the picker open (the overlay re-shows it
	 * with the refreshed focused states), so joining a second daily is not
	 * a close-and-reopen; presets still close on selection, like every
	 * other menu pick. */
	async function handleFocusQuestSelection(selection: OverlayMenuSelection) {
		suppressBlurCloseUntil = Date.now() + 400;
		await currentWindow.emitTo('overlay', OVERLAY_MENU_SELECT_EVENT, selection).catch(() => {});
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
		<div class="menu-panel">
			{#if menuState.kind === 'focus'}
				{#if menuState.quests.length === 0 && menuState.presets.length === 0}
					<div class="menu-empty">No quests to focus</div>
				{:else}
					{@const anyFocused = menuState.quests.some((quest) => quest.focused)}
					{#each menuState.quests as quest (quest.questId)}
						<div class="menu-row">
							<button
								type="button"
								class="menu-option {quest.focused ? 'menu-option-active' : ''}"
								title={quest.focused
									? 'Unfocus (the stretch ends here)'
									: quest.signalQuest
										? 'Start a run: play counts toward it until its signal loot drops'
										: 'Focus: play from now on counts toward this quest'}
								onclick={() =>
									handleFocusQuestSelection(
										quest.focused
											? { kind: 'focus', action: 'questUnfocus', questId: quest.questId }
											: { kind: 'focus', action: 'questFocus', questId: quest.questId, additive: false }
									)}
							>
								<span class="menu-option-name">{quest.name}</span>
								{#if quest.focused}
									<span class="menu-option-badge">Focused</span>
								{:else if quest.signalQuest}
									<!-- The standing, repeatable chip: focusing starts a
										 run; the signal loot ends it. -->
									<span class="menu-option-badge menu-option-badge-muted">Run</span>
								{/if}
							</button>
							{#if !quest.focused && anyFocused}
								<button
									type="button"
									class="menu-join-btn"
									aria-label={`Also focus ${quest.name}`}
									title="Also focus: the play ahead advances this quest too"
									onclick={() =>
										handleFocusQuestSelection({
											kind: 'focus',
											action: 'questFocus',
											questId: quest.questId,
											additive: true
										})}
								>+</button>
							{/if}
						</div>
					{/each}
					{#if menuState.presets.length > 0}
						<div class="menu-heading">Recent segments</div>
						{#each menuState.presets as label (label)}
							<button
								type="button"
								class="menu-option"
								title="Start a segment with this name (closes the current one)"
								onclick={() => handleSelection({ kind: 'focus', action: 'preset', label })}
							>
								<span class="menu-option-name">{label}</span>
							</button>
						{/each}
					{/if}
				{/if}
			{:else if menuState.kind === 'trifecta'}
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
			{:else if menuState.loading}
				<div class="menu-empty">Searching...</div>
			{:else if menuState.error}
				<div class="menu-empty">{menuState.error}</div>
			{:else if menuState.kind === 'name'}
				{#if menuState.suggestions.length === 0}
					{@const typed = menuState.query.trim()}
					<button
						type="button"
						class="menu-option"
						onclick={() => handleSelection({ kind: 'name', name: typed })}
					>
						<span class="menu-option-name">Press Enter to name it "{typed}"</span>
					</button>
				{:else}
					{#each menuState.suggestions as option}
						<button
							type="button"
							class="menu-option"
							onclick={() => handleSelection({ kind: 'name', name: option })}
						>
							<span class="menu-option-name">{option}</span>
						</button>
					{/each}
				{/if}
			{:else if menuState.kind === 'mob'}
				{#if menuState.mobSuggestions.length === 0}
					<div class="menu-empty">No matches</div>
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
			{/if}
		</div>
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

	.menu-option-badge-muted {
		background: rgba(255, 255, 255, 0.07);
		color: rgba(255, 255, 255, 0.45);
	}

	.menu-empty {
		padding: 8px 10px;
		color: rgba(255, 255, 255, 0.45);
		font-size: 11px;
	}

	.menu-row {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.menu-row .menu-option {
		flex: 1;
		min-width: 0;
	}

	.menu-join-btn {
		flex-shrink: 0;
		width: 22px;
		height: 22px;
		border-radius: 5px;
		border: 1px solid rgba(255, 255, 255, 0.15);
		background: rgba(255, 255, 255, 0.05);
		color: rgba(255, 255, 255, 0.5);
		font-size: 12px;
		line-height: 1;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 140ms ease;
	}
	.menu-join-btn:hover {
		background: rgba(56, 189, 248, 0.16);
		border-color: rgba(56, 189, 248, 0.4);
		color: rgba(186, 230, 253, 0.96);
	}

	.menu-heading {
		padding: 8px 8px 3px;
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: rgba(255, 255, 255, 0.3);
	}
</style>
