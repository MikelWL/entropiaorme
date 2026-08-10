<script lang="ts">
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import type { ConfidenceMode } from './treeCuttingModel.svelte';

	let {
		active,
		onchange,
	}: {
		active: ConfidenceMode;
		onchange: (id: string) => void;
	} = $props();

	const options: {
		id: ConfidenceMode;
		label: string;
		ariaLabel: string;
		title: string;
		labelClass: string;
	}[] = [
		{
			id: 'liquid',
			label: '✓',
			ariaLabel: 'High volume only',
			title: 'Use high-volume market prices only',
			labelClass: 'text-positive',
		},
		{
			id: 'liquidMiddling',
			label: '⚠',
			ariaLabel: 'High and medium volume',
			title: 'Use high- and medium-volume market prices',
			labelClass: 'text-warning',
		},
		{
			id: 'all',
			label: '!',
			ariaLabel: 'High, medium, and low volume',
			title: 'Use high-, medium-, and low-volume market prices',
			labelClass: 'text-error',
		},
	];
</script>

<SegmentedControl {options} {active} {onchange} />
