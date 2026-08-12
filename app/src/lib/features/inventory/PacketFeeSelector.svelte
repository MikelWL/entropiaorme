<script lang="ts">
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';

	let {
		value,
		onchange,
	}: {
		value: number;
		onchange: (value: number) => void | Promise<void>;
	} = $props();

	const presets = [5, 10, 15];
	const options = [
		...presets.map((preset) => ({ id: String(preset), label: `${preset}%` })),
		{ id: 'custom', label: 'Custom' },
	];
	let customOpen = $state(false);
	const active = $derived(customOpen ? 'custom' : String(value));
	let customValue = $state('7.5');

	$effect(() => {
		if (!presets.includes(value)) {
			customOpen = true;
			customValue = String(value);
		}
	});

	function applyCustom() {
		const next = Number(customValue);
		if (Number.isFinite(next) && next > 0 && next <= 100) {
			void onchange(next);
		} else {
			customValue = String(value);
		}
	}

	function select(id: string) {
		if (id === 'custom') {
			customOpen = true;
			return;
		}
		customOpen = false;
		void onchange(Number(id));
	}
</script>

<div class="flex items-center gap-1.5">
	<SegmentedControl {options} {active} onchange={select} />
	{#if active === 'custom'}
		<label class="flex items-center gap-1 text-xs text-text-secondary">
			<input
				type="number"
				aria-label="Custom fee cap percentage"
				min="0.1"
				max="100"
				step="0.1"
				class="h-7 w-16 rounded border border-border bg-surface px-2 text-right text-xs tabular-nums text-text outline-none transition-colors focus:border-accent"
				bind:value={customValue}
				onblur={applyCustom}
				onkeydown={(event) => {
					if (event.key === 'Enter') event.currentTarget.blur();
				}}
			/>
			<span>%</span>
		</label>
	{/if}
</div>
