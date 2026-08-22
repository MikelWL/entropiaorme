<script lang="ts">
	import InfoTip from './InfoTip.svelte';

	let { align = 'right' }: { align?: 'left' | 'right' } = $props();
</script>

<InfoTip label="What Reward MU is" {align}>
	<p class="text-xs font-semibold leading-relaxed text-text">Projected, not realised</p>
	<p class="mt-1 text-xs leading-relaxed text-text-secondary">
		The actual reward recorded at completion, valued from its current market data. Anything
		without usable market data, Universal Ammo included, stays at its own TT. Realised figures
		continue to use confirmed value only.
	</p>
</InfoTip>
