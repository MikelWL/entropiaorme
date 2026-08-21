<script lang="ts">
	import { Button, Input, Modal } from '$lib/components';
	import type { ProtectionModel } from './protectionModel.svelte';

	let { model }: { model: ProtectionModel } = $props();

	type Mode = 'ready' | 'scanning' | 'review' | 'saved';
	let mode = $state<Mode>('ready');
	let value = $state('');
	let source = $state<'ocr' | 'manual'>('manual');
	let rawText = $state<string | null>(null);
	let calibrated = $state(true);
	let scanError = $state<string | null>(null);
	let resetReason = $state('');

	const parsedValue = $derived.by(() => {
		const parsed = Number(value.trim().replace(',', '.'));
		return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
	});
	const baseline = $derived(model.observationSet?.latestObservation ?? null);
	const increased = $derived(
		parsedValue !== null && baseline !== null && parsedValue > baseline.ttValuePed + 0.0000001,
	);
	const canConfirm = $derived(
		parsedValue !== null &&
		(!increased || resetReason.trim().length > 0) &&
		!model.saving,
	);

	$effect(() => {
		void model.observationSet?.id;
		mode = 'ready';
		value = '';
		source = 'manual';
		rawText = null;
		calibrated = true;
		scanError = null;
		resetReason = '';
	});

	async function scan(): Promise<void> {
		mode = 'scanning';
		scanError = null;
		try {
			const result = await model.scan();
			calibrated = result.calibrated;
			rawText = result.rawText;
			if (result.error || result.valuePed === null) {
				scanError = result.error ?? 'No number was recognised';
				mode = 'review';
				source = 'manual';
				return;
			}
			value = result.valuePed.toFixed(2);
			source = 'ocr';
			mode = 'review';
		} catch {
			scanError = 'Trade Terminal scan failed';
			mode = 'review';
			source = 'manual';
		}
	}

	function enterManually(): void {
		source = 'manual';
		rawText = null;
		scanError = null;
		mode = 'review';
	}

	async function confirm(): Promise<void> {
		if (!canConfirm || parsedValue === null) return;
		const outcome = await model.confirmObservation({
			valuePed: parsedValue,
			source,
			rawText,
			resetReason: increased ? resetReason.trim() : null,
		});
		if (outcome) mode = 'saved';
	}

	function close(): void {
		model.closeObservation();
	}
</script>

<Modal
	bind:open={model.observationModalOpen}
	title={model.observationSet ? `Record ${model.observationSet.name}` : 'Record TT value'}
	class="max-w-xl"
>
	{#if model.observationSet}
		<div class="space-y-5">
			<div class="flex items-start justify-between gap-5 border-b border-border/70 pb-4">
				<div>
					<p class="text-sm text-text-secondary">
						Place exactly seven {model.observationSet.kind === 'armour' ? 'armour pieces' : 'plates'} in the Trade Terminal. Do not sell them.
					</p>
					{#if baseline}
						<p class="mt-1.5 text-xs text-text-tertiary">
							Current baseline: <span class="font-medium tabular-nums text-text-secondary">{baseline.ttValuePed.toFixed(2)} PED</span>
						</p>
					{:else}
						<p class="mt-1.5 text-xs text-text-tertiary">The first confirmed reading establishes the baseline and records no cost.</p>
					{/if}
				</div>
				<span class="shrink-0 text-xs font-medium text-accent">{model.observationSet.markupPercent?.toFixed(2)}% MU</span>
			</div>

			{#if mode === 'ready'}
				<div class="flex items-center gap-3">
					<Button onclick={scan}>Scan Trade Terminal</Button>
					<Button variant="secondary" onclick={enterManually}>Enter manually</Button>
				</div>
			{:else if mode === 'scanning'}
				<div class="py-7 text-center text-sm text-text-secondary animate-pulse">Reading the Trade Terminal total...</div>
			{:else if mode === 'saved'}
				{#if model.lastOutcome?.costWindow}
					{@const window = model.lastOutcome.costWindow}
					<div class="space-y-2">
						<p class="text-sm font-medium text-text">
							{window.status === 'booked' ? 'Protection cost allocated' : window.costKnown ? 'Measurement saved without matching evidence' : 'Prior protection cost needs correction'}
						</p>
						<div class="grid grid-cols-3 gap-4 border-y border-border/70 py-3">
							<div><span class="eyebrow block">TT consumed</span><span class="tabular-nums text-sm text-text">{window.costKnown ? `${window.consumedTtPed?.toFixed(4)} PED` : 'Unknown'}</span></div>
							<div><span class="eyebrow block">Markup basis</span><span class="tabular-nums text-sm text-text">{window.markupPercent?.toFixed(2)}%</span></div>
							<div><span class="eyebrow block">Cost</span><span class="tabular-nums text-sm font-semibold text-accent">{window.costKnown ? `${window.costPed.toFixed(4)} PED` : 'Unknown'}</span></div>
						</div>
						{#if window.allocations.length > 0}<p class="text-xs text-text-tertiary">Spread across {window.allocations.length} {window.allocations.length === 1 ? 'session' : 'sessions'} using recorded incoming damage.</p>{/if}
						{#if window.reason}<p class="text-xs text-warning">{window.reason}</p>{/if}
					</div>
				{:else}
					<div>
						<p class="text-sm font-medium text-text">Baseline established</p>
						<p class="mt-1 text-xs text-text-tertiary">The next confirmed reading will measure consumption from this value.</p>
					</div>
				{/if}
				<div class="flex justify-end"><Button onclick={close}>Done</Button></div>
			{:else}
				<div class="space-y-4">
					{#if !calibrated}
						<div class="border-l-2 border-warning pl-3 text-xs text-warning">
							Provisional coordinates were used. Check the recognised value carefully or enter it manually.
						</div>
					{/if}
					{#if scanError}
						<div class="border-l-2 border-warning pl-3 text-xs text-text-secondary">{scanError}. Enter the displayed value below.</div>
					{/if}
					<div>
						<label for="protection-tt-value" class="block eyebrow mb-1.5">Total TT value</label>
						<div class="flex items-center gap-2">
							<Input id="protection-tt-value" bind:value type="text" inputmode="decimal" placeholder="0.00" class="max-w-40" />
							<span class="text-sm text-text-tertiary">PED</span>
						</div>
						{#if rawText}<p class="mt-1 text-[11px] text-text-tertiary">OCR read: {rawText}</p>{/if}
					</div>

					{#if increased}
						<div class="border-l-2 border-warning pl-3 space-y-2">
							<p class="text-xs text-text-secondary">This is above the current baseline, so it cannot represent decay. Record it as a fresh baseline instead.</p>
							<label for="protection-reset-reason" class="block eyebrow">Why the baseline changed</label>
							<Input id="protection-reset-reason" bind:value={resetReason} placeholder="Pieces replaced, replenished, or earlier read corrected" />
						</div>
					{/if}

					<div class="flex items-center justify-between pt-1">
						<Button variant="ghost" onclick={scan} disabled={model.saving}>Re-scan</Button>
						<div class="flex gap-2">
							<Button variant="secondary" onclick={close} disabled={model.saving}>Cancel</Button>
							<Button onclick={confirm} disabled={!canConfirm} loading={model.saving}>
								{increased ? 'Reset baseline' : baseline ? 'Confirm measurement' : 'Set baseline'}
							</Button>
						</div>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</Modal>
