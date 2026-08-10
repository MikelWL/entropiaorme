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

<div class="flex items-center gap-2.5">
	<span class="eyebrow">MU conf</span>
	<InfoTip label="How markup confidence works">
		<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
			<p class="font-semibold text-text">Markup confidence: Choose which market prices to use</p>
			<p>
				Each level uses the item's markup, how much TT value has sold, how recent those sales
				are, and whether the markup can cover the auction fee.
			</p>
			<ul class="space-y-1.5">
				<li>
					<span class="font-semibold text-positive" aria-label="High volume">✓</span> Enough TT value sells each week to
					make the markup practical to realise.
				</li>
				<li>
					<span class="font-semibold text-warning" aria-label="Medium volume">⚠</span> Sales are less frequent, but the
					markup is high enough for a practical sale to cover the 0.5 PED minimum fee.
				</li>
				<li>
					<span class="font-semibold text-error" aria-label="Low volume">!</span> Too little TT value has sold recently
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
