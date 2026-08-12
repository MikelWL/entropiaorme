<!--
	The sale-window capture button, reachable while the game holds the screen.

	It is the main window's button in another place, and deliberately nothing
	more: no fields, no editing, no result to read. What it says is only
	whether to try again before tabbing away, because a read that failed
	because the window drifted off the corner is fixed in a second while the
	game is still in front, and discovering it later costs exactly the trip
	this window exists to save. The figures themselves are reviewed in the
	form, which is where they can be corrected and where they are committed.
-->
<script lang="ts">
	import { captureSaleWindow } from '$lib/api/inventory';
	import { hideSaleCaptureOverlay } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import { createWindowSizeSync } from '$lib/windows/windowSize';
	import { onMount } from 'svelte';

	let overlayRoot: HTMLDivElement | null = $state(null);
	const windowSizeSync = createWindowSizeSync(() => overlayRoot);

	// The window is sized to whatever the panel measures, so the result line
	// appearing does not leave it clipped.
	onMount(() => {
		windowSizeSync.schedule();
		const observer = new ResizeObserver(() => windowSizeSync.schedule());
		if (overlayRoot) observer.observe(overlayRoot);
		return () => {
			observer.disconnect();
			windowSizeSync.cancel();
		};
	});

	let busy = $state(false);
	let message = $state('');
	let failed = $state(false);

	async function capture() {
		if (busy) return;
		busy = true;
		message = '';
		try {
			const read = await captureSaleWindow();
			failed = read.error !== null;
			message = read.error ?? 'Captured. Check the main window.';
		} catch (cause) {
			failed = true;
			message = cause instanceof Error ? cause.message : 'Could not read the sale window';
		} finally {
			busy = false;
		}
	}
</script>

<div
	bind:this={overlayRoot}
	class="bg-surface-raised/95 text-text-primary flex w-[240px] flex-col gap-2 rounded-md p-3 shadow-lg backdrop-blur"
>
	<div class="flex items-center gap-2">
		<div data-tauri-drag-region class="text-text-tertiary flex-1 cursor-move text-xs">
			Capture listing
		</div>
		<button
			type="button"
			class="text-text-tertiary hover:text-text-primary text-xs leading-none"
			onclick={() => hideSaleCaptureOverlay()}
			aria-label="Close"
		>
			✕
		</button>
	</div>

	<Button variant="secondary" size="sm" onclick={capture} disabled={busy}>
		{busy ? 'Reading the window' : 'Capture from game'}
	</Button>

	{#if message !== ''}
		<p class="text-xs {failed ? 'text-status-danger' : 'text-text-tertiary'}">{message}</p>
	{/if}
</div>
