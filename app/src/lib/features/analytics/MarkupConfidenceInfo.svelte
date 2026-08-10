<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
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
					<span class="font-semibold text-positive" aria-hidden="true">✓</span>
					Enough TT value sells each week to make the markup practical to realise.
				</li>
				<li>
					<span class="font-semibold text-warning" aria-hidden="true">⚠</span>
					Sales are less frequent, but the markup is high enough for a practical sale to cover
					the 0.5 PED minimum fee.
				</li>
				<li>
					<span class="font-semibold text-error" aria-hidden="true">!</span>
					Too little TT value has sold recently to rely on the markup.
				</li>
			</ul>
			<p>
				Items excluded by the selected confidence level use the Nanocube markup instead. The
				amount you currently hold does not affect these levels.
			</p>
		</div>
	</InfoTip>
</div>
