<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
</script>

<div class="flex items-center gap-2.5">
	<span class="eyebrow">Fee cap</span>
	<InfoTip label="How the packet fee cap works">
		<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
			<p class="font-semibold text-text">Fee cap: Limit the listing fee's share of gross markup</p>
			<p>
				The packet TT is the smallest amount whose expected markup keeps the auction listing fee
				within this percentage. A lower cap preserves more markup but requires a larger packet.
			</p>
			<p>
				The estimate uses the item's observed markup and the measured auction fee curve. It does not
				limit the packet to recent market volume.
			</p>
		</div>
	</InfoTip>
</div>
