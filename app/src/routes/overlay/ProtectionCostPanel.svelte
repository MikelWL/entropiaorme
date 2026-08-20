<script lang="ts">
	import { untrack } from 'svelte';
	import {
		confirmProtectionObservation,
		confirmProtectionRepair,
		scanRepairCost,
		scanTradeTerminalValue,
		type ProtectionObservationOutcome,
		type ProtectionCostWindow,
	} from '$lib/api';
	import type { ProtectionCostStep } from '$lib/features/protection/protectionCostFlow';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';

	interface Props {
		sessionId: string;
		repairOcrEnabled: boolean;
		steps: ProtectionCostStep[];
		onClose: () => void;
	}

	let { sessionId, repairOcrEnabled, steps, onClose }: Props = $props();

	type Mode = 'ready' | 'scanning' | 'review' | 'saved';
	let stepIndex = $state(0);
	let mode = $state<Mode>('ready');
	let value = $state('');
	let source = $state<'ocr' | 'manual'>('manual');
	let rawText = $state<string | null>(null);
	let calibrated = $state(true);
	let errorHint = $state<string | null>(null);
	let saving = $state(false);
	let asserted = $state(false);
	let resetReason = $state('');
	let limitedOutcome = $state<ProtectionObservationOutcome | null>(null);
	let savedRepairCost = $state<number | null>(null);
	let savedWindow = $state<ProtectionCostWindow | null>(null);
	const tokens = untrack(() =>
		steps.map(
			(_, index) =>
				globalThis.crypto?.randomUUID?.() ??
				`protection-cost-${Date.now()}-${index}-${Math.random().toString(36).slice(2)}`,
		),
	);

	const step = $derived(steps[stepIndex]);
	const lastStep = $derived(stepIndex === steps.length - 1);
	const parsedValue = $derived.by(() => {
		const parsed = Number(value.trim().replace(',', '.'));
		return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
	});
	const increased = $derived(
		step?.method === 'limited' &&
		step.baselineTtPed !== null &&
		parsedValue !== null &&
		parsedValue > step.baselineTtPed + 0.0000001,
	);
	const requiresAssertion = $derived(
		step?.method === 'limited' && step.baselineTtPed !== null && !increased,
	);
	const canConfirm = $derived(
		parsedValue !== null &&
		(!requiresAssertion || asserted) &&
		(!increased || resetReason.trim().length > 0) &&
		!saving,
	);

	function layerLabel(layer: ProtectionCostStep['layer']): string {
		if (layer === 'armour') return 'Armour';
		if (layer === 'plates') return 'Plates';
		return 'Armour + plates';
	}

	function instruction(current: ProtectionCostStep): string {
		if (current.layer === 'combined') {
			return 'Place all equipped armour and plates in the Repair Terminal.';
		}
		const items = current.layer === 'armour' ? 'seven armour pieces' : 'seven plates';
		const terminal = current.method === 'limited' ? 'Trade Terminal' : 'Repair Terminal';
		return `Place the ${items} in the ${terminal}. Do not complete the transaction.`;
	}

	function resetEntry(): void {
		mode = 'ready';
		value = '';
		source = 'manual';
		rawText = null;
		calibrated = true;
		errorHint = null;
		asserted = false;
		resetReason = '';
		limitedOutcome = null;
		savedRepairCost = null;
		savedWindow = null;
	}

	async function scan(): Promise<void> {
		if (!step) return;
		mode = 'scanning';
		errorHint = null;
		try {
			if (step.method === 'limited') {
				const result = await scanTradeTerminalValue();
				calibrated = result.calibrated;
				rawText = result.rawText;
				if (result.error || result.valuePed === null) {
					errorHint = result.error ?? 'No number was recognised';
					mode = 'review';
					source = 'manual';
					return;
				}
				value = result.valuePed.toFixed(2);
			} else {
				const result = await scanRepairCost(sessionId);
				if (result.error || result.cost_ped == null) {
					errorHint = result.error ?? 'No number was recognised';
					mode = 'review';
					source = 'manual';
					return;
				}
				rawText = result.raw_text ?? null;
				value = result.cost_ped.toFixed(2);
			}
			source = 'ocr';
			mode = 'review';
		} catch {
			errorHint = `${step.method === 'limited' ? 'Trade Terminal' : 'Repair Terminal'} scan failed`;
			source = 'manual';
			mode = 'review';
		}
	}

	function enterManually(): void {
		source = 'manual';
		rawText = null;
		errorHint = null;
		mode = 'review';
	}

	async function confirm(): Promise<void> {
		if (!step || !canConfirm || parsedValue === null) return;
		saving = true;
		errorHint = null;
		try {
			if (step.method === 'limited' && step.setId) {
				limitedOutcome = await confirmProtectionObservation({
					setId: Number(step.setId),
					clientToken: tokens[stepIndex],
					ttValuePed: parsedValue,
					source,
					rawText,
					resetReason: increased ? resetReason.trim() : null,
				});
			} else {
				const outcome = await confirmProtectionRepair({
					clientToken: tokens[stepIndex],
					armourSetId: step.armourSetId ? Number(step.armourSetId) : null,
					plateSetId: step.plateSetId ? Number(step.plateSetId) : null,
					costPed: parsedValue,
				});
				savedWindow = outcome.costWindow;
				savedRepairCost = parsedValue;
			}
			mode = 'saved';
		} catch (error) {
			errorHint = error instanceof Error ? error.message : 'Protection cost could not be saved';
		} finally {
			saving = false;
		}
	}

	function continueFlow(): void {
		if (lastStep) {
			onClose();
			return;
		}
		stepIndex += 1;
		resetEntry();
	}
</script>

{#if step}
	<div class="flex min-w-[390px] flex-col gap-3 text-white">
		<div class="flex items-start justify-between gap-6 border-b border-white/10 pb-2.5">
			<div>
				<div class="flex items-center gap-2">
					<span class="text-xs font-semibold">{layerLabel(step.layer)}</span>
					<span class="text-[10px] uppercase tracking-wider text-white/35">{step.method === 'limited' ? 'Limited' : 'Unlimited'}</span>
				</div>
				<p class="mt-0.5 text-[11px] text-white/45">{step.name}</p>
			</div>
			<div class="flex items-center gap-2">
				{#if steps.length > 1}<span class="text-[10px] tabular-nums text-white/35">{stepIndex + 1} of {steps.length}</span>{/if}
				<Button variant="ghost" size="sm" aria-label="Record protection cost later" onclick={onClose}>Later</Button>
			</div>
		</div>

		{#if mode === 'saved'}
			<div class="flex items-center justify-between gap-6">
				<div>
					<p class="text-xs font-medium">
						{#if limitedOutcome?.costWindow}
							{limitedOutcome.costWindow.status === 'booked' ? 'Protection cost allocated' : 'Measurement saved for later allocation'}
						{:else if limitedOutcome}
							Baseline established
						{:else if savedWindow}
							{savedWindow.status === 'booked' ? 'Repair cost allocated' : 'Repair cost saved without matching evidence'}
						{:else}
							{savedRepairCost?.toFixed(2)} PED repair cost recorded
						{/if}
					</p>
					{#if limitedOutcome?.costWindow}
						<p class="mt-1 text-[11px] text-white/45">
							{limitedOutcome.costWindow.consumedTtPed?.toFixed(4)} TT at {limitedOutcome.costWindow.markupPercent?.toFixed(2)}% MU = {limitedOutcome.costWindow.costPed.toFixed(4)} PED
						</p>
						{#if limitedOutcome.costWindow.allocations.length > 0}<p class="mt-1 text-[10px] text-white/35">Spread across {limitedOutcome.costWindow.allocations.length} {limitedOutcome.costWindow.allocations.length === 1 ? 'session' : 'sessions'} from recorded damage.</p>{/if}
					{:else if savedWindow?.allocations.length}
						<p class="mt-1 text-[11px] text-white/45">{savedWindow.costPed.toFixed(4)} PED spread across {savedWindow.allocations.length} {savedWindow.allocations.length === 1 ? 'session' : 'sessions'} from recorded damage.</p>
					{/if}
				</div>
				<Button size="sm" onclick={continueFlow}>{lastStep ? 'Done' : `Continue to ${layerLabel(steps[stepIndex + 1].layer).toLowerCase()}`}</Button>
			</div>
		{:else}
			<p class="text-[11px] text-white/55">{instruction(step)}</p>

			{#if mode === 'ready'}
				<div class="flex items-center gap-2">
					{#if step.method === 'limited' || repairOcrEnabled}
						<Button size="sm" onclick={scan}>Scan {step.method === 'limited' ? 'Trade Terminal' : 'Repair Terminal'}</Button>
					{/if}
					<Button variant="secondary" size="sm" onclick={enterManually}>Enter manually</Button>
					{#if step.method === 'limited' && step.baselineTtPed !== null}
						<span class="ml-auto text-[10px] text-white/35">Baseline {step.baselineTtPed.toFixed(2)} PED</span>
					{/if}
				</div>
			{:else if mode === 'scanning'}
				<div class="py-2 text-center text-xs text-white/45 animate-pulse">Reading terminal value...</div>
			{:else}
				<div class="flex items-center gap-2">
					<Input class="w-28" bind:value type="text" inputmode="decimal" placeholder="0.00 PED" disabled={saving} />
					{#if rawText}<span class="max-w-[100px] truncate text-[10px] text-white/30" title={rawText}>OCR: {rawText}</span>{/if}
					<Button size="sm" onclick={confirm} disabled={!canConfirm} loading={saving}>{increased ? 'Reset baseline' : step.method === 'limited' && step.baselineTtPed === null ? 'Set baseline' : 'Confirm'}</Button>
					<Button variant="ghost" size="sm" onclick={scan} disabled={saving}>Re-scan</Button>
				</div>

				{#if step.method === 'limited' && !calibrated}
					<p class="border-l-2 border-amber-400/70 pl-2 text-[10px] text-amber-200/80">Provisional coordinates were used. Check the value or enter it manually.</p>
				{/if}
				{#if increased}
					<div class="flex items-center gap-2 border-l-2 border-amber-400/70 pl-2">
						<span class="text-[10px] text-white/45">Above the baseline. Explain the reset:</span>
						<Input class="min-w-52" bind:value={resetReason} placeholder="Pieces replaced or reading corrected" />
					</div>
				{:else if requiresAssertion}
					<label class="flex items-start gap-2 text-[10px] text-white/45">
						<input class="mt-0.5 accent-accent" type="checkbox" bind:checked={asserted} />
						<span>This set was not replaced and was used only while EntropiaOrme was recording between these readings.</span>
					</label>
				{/if}
			{/if}

			{#if errorHint}<p class="border-l-2 border-amber-400/70 pl-2 text-[10px] text-amber-200/80">{errorHint}</p>{/if}
		{/if}
	</div>
{/if}
