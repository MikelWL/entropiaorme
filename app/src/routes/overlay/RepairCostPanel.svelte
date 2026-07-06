<script lang="ts">
	import { untrack } from 'svelte';
	import { scanRepairCost, saveArmourCost } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';

	interface Props {
		sessionId: string;
		repairOcrEnabled: boolean;
		onClose: () => void;
	}

	let { sessionId, repairOcrEnabled, onClose }: Props = $props();

	type Mode = 'idle' | 'scanning' | 'result' | 'manual';

	// OCR disabled → permanent manual mode. OCR enabled → start at idle.
	let mode = $state<Mode>(untrack(() => (repairOcrEnabled ? 'idle' : 'manual')));
	let result = $state<{ cost_ped: number; raw_text: string } | null>(null);
	let errorHint = $state<string | null>(null);
	let manualCost = $state('');
	let saving = $state(false);

	const manualCostValue = $derived.by(() => {
		const normalised = manualCost.trim().replace(',', '.');
		if (!normalised) return null;
		const value = Number(normalised);
		if (!Number.isFinite(value) || value < 0) return null;
		return value;
	});

	function formatPed(v: number): string {
		return v.toFixed(2);
	}

	async function handleScan() {
		mode = 'scanning';
		errorHint = null;
		try {
			const scan = await scanRepairCost(sessionId);
			if (scan.error) {
				errorHint = scan.error;
				result = null;
				mode = 'manual';
			} else {
				result = { cost_ped: scan.cost_ped, raw_text: scan.raw_text };
				mode = 'result';
			}
		} catch {
			errorHint = 'Scan failed';
			result = null;
			mode = 'manual';
		}
	}

	function handleEnterManually() {
		errorHint = null;
		mode = 'manual';
	}

	async function handleSave(cost: number) {
		saving = true;
		try {
			await saveArmourCost(sessionId, cost);
		} catch { /* ignore */ }
		saving = false;
		onClose();
	}

	async function handleSubmitManual() {
		if (manualCostValue == null) return;
		await handleSave(manualCostValue);
	}

	function handleManualKeydown(event: KeyboardEvent) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		void handleSubmitManual();
	}
</script>

<div class="flex items-center gap-2 shrink-0">
	{#if mode === 'scanning'}
		<span class="text-xs text-white/50 animate-pulse px-2">Reading repair cost...</span>
	{:else if mode === 'result' && result}
		<span class="text-xs text-white/50">Repair:</span>
		<span class="text-sm font-semibold text-white tabular-nums">{formatPed(result.cost_ped)} PED</span>
		<span
			class="text-[10px] text-white/30 truncate max-w-[80px] ml-1"
			title={result.raw_text}
		>OCR: {result.raw_text}</span>
		<Button variant="secondary" size="sm" onclick={handleScan} disabled={saving}>Re-scan</Button>
		<Button
			variant="primary"
			size="sm"
			onclick={() => handleSave(result!.cost_ped)}
			disabled={saving}
		>Confirm</Button>
		<Button variant="secondary" size="sm" onclick={handleEnterManually} disabled={saving}>Enter manually</Button>
	{:else if mode === 'manual'}
		<span class="text-xs text-white/50">Repair:</span>
		<Input
			class="w-24"
			bind:value={manualCost}
			type="text"
			inputmode="decimal"
			placeholder="0.00 PED"
			disabled={saving}
			onkeydown={handleManualKeydown}
		/>
		{#if errorHint}
			<span
				class="text-[10px] text-white/40 truncate max-w-[120px]"
				title={errorHint}
			>OCR: {errorHint}</span>
		{/if}
		<Button
			variant="primary"
			size="sm"
			onclick={handleSubmitManual}
			disabled={saving || manualCostValue == null}
		>Save</Button>
		{#if repairOcrEnabled}
			<Button variant="secondary" size="sm" onclick={handleScan} disabled={saving}>Scan OCR</Button>
		{/if}
	{:else}
		<span class="text-xs text-white/50">Armour cost:</span>
		<Button variant="primary" size="sm" onclick={handleScan}>Record</Button>
		<Button variant="secondary" size="sm" onclick={handleEnterManually}>Enter manually</Button>
	{/if}
</div>
