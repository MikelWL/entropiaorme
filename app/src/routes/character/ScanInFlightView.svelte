<script lang="ts">
	import { onMount } from 'svelte';
	import {
		cancelManualSkillScan,
		setSpacebarCapture,
		type ScanManualStatus,
	} from '$lib/api';
	import ScanGallery from './ScanGallery.svelte';
	import ScanProcessingButton from './ScanProcessingButton.svelte';
	import ScanReviewPane from './ScanReviewPane.svelte';

	let {
		status,
		onComplete,
	}: {
		status: ScanManualStatus;
		onComplete: () => void;
	} = $props();

	let cancelBusy = $state(false);
	let cancelError = $state<string | null>(null);

	// Shared key with scan-overlay/+page.svelte so toggling here propagates on
	// the overlay's next mount; the backend listener is a single global so
	// state is coherent regardless of which surface drives the toggle.
	const SPACEBAR_CAPTURE_KEY = 'scan-overlay:spacebar-capture';

	function readStoredSpacebarCapture(): boolean {
		if (typeof localStorage === 'undefined') return false;
		return localStorage.getItem(SPACEBAR_CAPTURE_KEY) === 'true';
	}

	let spacebarCapture = $state<boolean>(readStoredSpacebarCapture());
	let spacebarError = $state<string | null>(null);

	$effect(() => {
		if (typeof localStorage === 'undefined') return;
		localStorage.setItem(SPACEBAR_CAPTURE_KEY, String(spacebarCapture));
	});

	async function syncSpacebarCapture(enabled: boolean) {
		spacebarError = null;
		try {
			await setSpacebarCapture(enabled);
		} catch (err) {
			spacebarError = `spacebar toggle failed: ${err instanceof Error ? err.message : String(err)}`;
		}
	}

	async function toggleSpacebarCapture() {
		spacebarCapture = !spacebarCapture;
		await syncSpacebarCapture(spacebarCapture);
	}

	onMount(async () => {
		// Replay stored state to the backend so the listener matches the UI
		// even if the user enters this view via a route that skipped the
		// overlay's onMount sync.
		await syncSpacebarCapture(spacebarCapture);
	});

	async function cancel() {
		cancelBusy = true;
		cancelError = null;
		try {
			const result = await cancelManualSkillScan();
			if ('error' in result && result.error) {
				cancelError = result.error;
			}
		} catch (err) {
			cancelError = err instanceof Error ? err.message : String(err);
		} finally {
			cancelBusy = false;
		}
	}

	let phaseLabel = $derived(
		status.phase === 'capturing'
			? 'Capturing'
			: status.phase === 'processing'
				? 'Processing'
				: status.phase === 'awaiting_review'
					? 'Awaiting review'
					: 'Idle'
	);

	let progressFraction = $derived.by(() => {
		if (status.phase === 'capturing') {
			return status.expected_pages > 0 ? status.captured_pages / status.expected_pages : 0;
		}
		if (status.phase === 'processing') {
			const { done, total } = status.processing_progress;
			return total > 0 ? done / total : 0;
		}
		return 1;
	});

	let progressText = $derived.by(() => {
		if (status.phase === 'capturing') {
			return `${status.captured_pages} / ${status.expected_pages} captured`;
		}
		if (status.phase === 'processing') {
			const { done, total } = status.processing_progress;
			return `${done} / ${total} pages processed`;
		}
		return '';
	});

	let allCaptured = $derived(status.captured_pages >= status.expected_pages);
</script>

<div class="space-y-4">
	<div class="flex items-baseline justify-between gap-4">
		<div class="flex items-baseline gap-3">
			<span class="text-xs font-medium uppercase tracking-wide text-text-tertiary">{phaseLabel}</span>
			<span class="text-sm text-text">Skills scan</span>
			{#if progressText}
				<span class="text-xs text-text-secondary tabular-nums">{progressText}</span>
			{/if}
		</div>
		<div class="flex items-center gap-2">
			{#if status.phase === 'capturing' || status.phase === 'awaiting_review'}
				<button
					type="button"
					class="rounded bg-surface px-2.5 py-1 text-xs text-text-secondary transition-colors hover:bg-surface-hover hover:text-text disabled:opacity-50 cursor-pointer"
					onclick={cancel}
					disabled={cancelBusy}
				>
					{status.phase === 'awaiting_review' ? 'Discard' : 'Cancel'}
				</button>
			{/if}
		</div>
	</div>

	{#if status.phase !== 'awaiting_review'}
		<div class="h-1 overflow-hidden rounded-full bg-surface">
			<div
				class="h-full bg-accent/60 transition-[width] duration-150 ease-out"
				style="width: {Math.min(100, Math.max(0, progressFraction * 100))}%"
			></div>
		</div>
	{/if}

	{#if cancelError}
		<p class="text-xs text-warning">{cancelError}</p>
	{/if}

	{#if status.phase === 'capturing'}
		<ScanGallery captured={status.captured_pages} expected={status.expected_pages} />
		{#if allCaptured}
			<ScanProcessingButton />
			<p class="text-center text-xs text-text-tertiary">
				All {status.expected_pages} pages captured. Click Start Processing to extract skill levels.
			</p>
		{:else}
			<div class="flex flex-col items-center gap-1.5">
				<p class="text-center text-xs text-text-tertiary">
					Capture each page from the overlay. Click a thumbnail to verify the crop.
				</p>
				<label class="inline-flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
					<input
						type="checkbox"
						checked={spacebarCapture}
						onchange={toggleSpacebarCapture}
						class="cursor-pointer"
					/>
					<span>Capture with <kbd class="rounded border border-border bg-surface px-1 text-[10px] font-medium text-text">Spacebar</kbd></span>
				</label>
				{#if spacebarError}
					<p class="text-[10px] text-warning">{spacebarError}</p>
				{/if}
			</div>
		{/if}
	{:else if status.phase === 'processing'}
		<ScanGallery captured={status.captured_pages} expected={status.expected_pages} dimmed />
		<p class="text-center text-xs text-text-tertiary">
			Reading skill levels from each page. Hang tight…
		</p>
	{:else if status.phase === 'awaiting_review'}
		<ScanReviewPane {onComplete} />
	{/if}
</div>
