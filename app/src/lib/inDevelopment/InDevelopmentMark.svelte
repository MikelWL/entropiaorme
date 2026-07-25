<script lang="ts">
	// The one marker for a surface that ships ahead of its capability. Sits
	// inside a block already gated on `inDevelopment.visible`, so it does not
	// re-check the channel: its job is to say *why* the surface is inert, not
	// whether it appears.
	//
	// One implementation on purpose. A second one drifts, and a drifted
	// marker is worse than none because it teaches the reader that the mark
	// is decoration.
	import InfoTip from '$lib/components/InfoTip.svelte';
	import { inDevelopmentSurface } from './registry';

	let {
		id,
		align = 'right',
	}: {
		/** A key registered in `IN_DEVELOPMENT_SURFACES`. */
		id: string;
		align?: 'left' | 'right';
	} = $props();

	const surface = $derived(inDevelopmentSurface(id));
</script>

<InfoTip {align} width="w-80" label="In development: {surface.summary}">
	{#snippet trigger()}
		<span
			class="inline-flex items-center rounded border border-warning/40 bg-warning/10
				px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning"
		>
			In development
		</span>
	{/snippet}
	<p class="text-xs leading-relaxed text-text-secondary">{surface.summary}</p>
	<p class="mt-2 text-xs leading-relaxed text-text-tertiary">{surface.graduates}</p>
</InfoTip>
