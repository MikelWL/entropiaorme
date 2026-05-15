<script lang="ts">
	import { processManualSkillScan } from '$lib/api';
	import Button from '$lib/components/Button.svelte';

	let { disabled = false }: { disabled?: boolean } = $props();

	let busy = $state(false);
	let error = $state<string | null>(null);

	async function startProcessing() {
		busy = true;
		error = null;
		try {
			const result = await processManualSkillScan();
			if (result.error) error = result.error;
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			busy = false;
		}
	}
</script>

<div class="flex flex-col items-center gap-2 py-2">
	<Button onclick={startProcessing} disabled={disabled || busy}>
		{#snippet children()}{busy ? 'Starting…' : 'Start Processing'}{/snippet}
	</Button>
	{#if error}
		<p class="text-xs text-warning">{error}</p>
	{/if}
</div>
