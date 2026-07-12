<script lang="ts">
	// The market-data preference blocks on the Settings page: the
	// download-only snapshot fetch, and the strictly separate,
	// default-off contribution opt-in with its token. Contribution
	// stays inert without both the toggle and a token.
	import { Divider, Input, Toggle } from '$lib/components';
	import {
		marketContributionOptIn,
		marketContributorToken,
		marketDataOptIn,
		setMarketContributionOptIn,
		setMarketContributorToken,
		setMarketDataOptIn,
	} from '$lib/marketData.svelte';

	let tokenDraft = $state(marketContributorToken.current);

	async function saveToken() {
		await setMarketContributorToken(tokenDraft.trim());
	}
</script>

<Divider />

<!-- Market data (download) -->
<div class="py-5 flex items-start justify-between gap-6">
	<div>
		<p class="text-sm text-text">Market data</p>
		<p class="text-xs text-text-tertiary mt-0.5">
			On by default. On launch the app fetches the shared market snapshot from
			<code>market-data.entropiaorme.com</code>. Download-only; nothing about you or your
			data is sent. Estimated markup stays informational and never enters your recorded
			results.
		</p>
	</div>
	<Toggle
		checked={marketDataOptIn.current}
		onchange={setMarketDataOptIn}
		label="Enable market data"
	/>
</div>

<Divider />

<!-- Market contribution (upload; strictly opt-in) -->
<div class="py-5 flex items-start justify-between gap-6">
	<div>
		<p class="text-sm text-text">Contribute market data</p>
		<p class="text-xs text-text-tertiary mt-0.5">
			Off by default. When enabled, you can send a market paste you have accepted (public
			auction-house figures, nothing else) to the shared snapshot service. Sending is always
			a button you press, never automatic, and requires a contributor token.
		</p>
		{#if marketContributionOptIn.current}
			<div class="mt-3 max-w-xs">
				<Input
					type="password"
					placeholder="Contributor token"
					bind:value={tokenDraft}
					onblur={saveToken}
					aria-label="Contributor token"
				/>
			</div>
		{/if}
	</div>
	<Toggle
		checked={marketContributionOptIn.current}
		onchange={setMarketContributionOptIn}
		label="Enable market-data contribution"
	/>
</div>
