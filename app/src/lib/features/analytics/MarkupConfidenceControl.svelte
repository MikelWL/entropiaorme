<script lang="ts">
	import MarkupConfidenceInfo from './MarkupConfidenceInfo.svelte';
	import MarkupConfidenceSelector from './MarkupConfidenceSelector.svelte';
	import type { ConfidenceMode } from './treeCuttingModel.svelte';

	let {
		active,
		onchange,
	}: {
		active: ConfidenceMode;
		onchange: (id: string) => void;
	} = $props();

</script>

<div class="flex items-center gap-2.5">
	<MarkupConfidenceInfo />
	<MarkupConfidenceSelector {active} {onchange} />
</div>
