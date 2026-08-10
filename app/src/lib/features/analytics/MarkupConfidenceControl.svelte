<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import type { ConfidenceMode } from './treeCuttingModel.svelte';

	let {
		active,
		onchange,
	}: {
		active: ConfidenceMode;
		onchange: (id: string) => void;
	} = $props();

	const options: { id: ConfidenceMode; label: string }[] = [
		{ id: 'liquid', label: 'High Vol. Only' },
		{ id: 'liquidMiddling', label: 'High & Mid Vol.' },
		{ id: 'all', label: 'High, Mid & Low Vol.' },
	];
</script>

<div class="flex items-center gap-2.5">
	<span class="eyebrow">Markup confidence</span>
	<InfoTip label="How markup confidence works">
		<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
			<p class="font-semibold text-text">Markup confidence: Choose which market prices to use</p>
			<p>
				Each level uses the item's markup, how much TT value has sold, how recent those sales
				are, and whether the markup can cover the auction fee.
			</p>
			<ul class="space-y-1.5">
				<li>
					<span class="font-medium text-text">High Vol.</span> Enough TT value sells each week to
					make the markup practical to realise.
				</li>
				<li>
					<span class="font-medium text-text">Mid Vol.</span> Sales are less frequent, but the
					markup is high enough for a practical sale to cover the 0.5 PED minimum fee.
				</li>
				<li>
					<span class="font-medium text-text">Low Vol.</span> Too little TT value has sold recently
					to rely on the markup.
				</li>
			</ul>
			<p>
				Excluded items use the Nanocube markup instead. The amount you currently hold does not
				affect these levels.
			</p>
		</div>
	</InfoTip>
	<SegmentedControl {options} {active} {onchange} />
</div>
