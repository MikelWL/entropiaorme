<script lang="ts">
	import MarkupConfidenceInfo from './MarkupConfidenceInfo.svelte';
	import MarkupConfidenceSelector from './MarkupConfidenceSelector.svelte';
	import type { ConfidenceMode } from './treeCuttingModel.svelte';

	let {
		active,
		onchange,
	}: {
		active: ConfidenceMode;
		onchange: (id: ConfidenceMode) => void;
	} = $props();

	function selectConfidence(id: string) {
		onchange(id as ConfidenceMode);
	}

</script>

<div class="flex items-center gap-2.5">
	<MarkupConfidenceInfo />
	<MarkupConfidenceSelector {active} onchange={selectConfidence} />
</div>
