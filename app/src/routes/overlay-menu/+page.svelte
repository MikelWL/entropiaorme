<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import type { UnlistenFn } from '@tauri-apps/api/event';
	import OverlayMenuPanel from '$lib/components/overlay/OverlayMenuPanel.svelte';
	import {
		OVERLAY_MENU_CLOSED_EVENT,
		OVERLAY_MENU_HIDE_EVENT,
		OVERLAY_MENU_INTERACT_EVENT,
		OVERLAY_MENU_READY_EVENT,
		OVERLAY_MENU_SELECT_EVENT,
		OVERLAY_MENU_SHOW_EVENT,
		computeMenuHeight,
		menuRowCount,
		type OverlayMenuSelection,
		type OverlayMenuState
	} from '$lib/windows/overlayMenu';

	const currentWindow = getCurrentWebviewWindow();

	let menuState = $state<OverlayMenuState | null>(null);
	let suppressBlurCloseUntil = 0;

	const popupHeight = $derived(menuState ? computeMenuHeight(menuRowCount(menuState)) : 1);
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
		<OverlayMenuPanel
			state={menuState}
			onSelect={handleSelection}
			onFocusSelect={handleFocusQuestSelection}
		/>
	</div>
{/if}

<style>
	.overlay-menu-shell {
		display: flex;
		align-items: stretch;
		justify-content: stretch;
	}
</style>
