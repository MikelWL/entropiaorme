<script lang="ts">
	import { formatPercent } from '$lib/utils/format';
	import InfoTip from './InfoTip.svelte';

	let {
		looterLevel = null,
		coverage = null,
		incomplete = false,
	}: {
		looterLevel?: number | null;
		coverage?: number | null;
		incomplete?: boolean;
	} = $props();
</script>

<InfoTip label="What Expected Return includes" width="w-96">
	<p class="text-xs font-semibold leading-relaxed text-text">Offensive spend only</p>
	<p class="mt-1 text-xs leading-relaxed text-text-secondary">
		Models weapon and amplifier spend with known Efficiency. Healing, armour, harvesting, and
		other unmodelled costs are excluded because their return mechanics are not yet known. This is
		not a whole-activity forecast.
	</p>
	{#if looterLevel !== null || coverage !== null || incomplete}
		<p class="mt-2 text-[11px] leading-relaxed text-text-tertiary">
			Community model v1
			{#if looterLevel !== null}
				· three-looter mean {looterLevel.toFixed(1)}
			{/if}
			{#if coverage !== null}
				· {formatPercent(coverage)} offensive basis coverage
			{/if}
			{#if incomplete}
				· partial basis
			{/if}
		</p>
	{/if}
</InfoTip>
