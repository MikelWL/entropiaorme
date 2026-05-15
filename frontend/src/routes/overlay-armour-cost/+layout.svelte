<script lang="ts">
	let { children } = $props();
</script>

<div class="overlay-armour-cost-root select-none">
	{@render children()}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		overflow: hidden !important;
		margin: 0 !important;
		padding: 0 !important;
		width: 100% !important;
		height: 100% !important;
		min-width: 0 !important;
		min-height: 0 !important;
	}

	.overlay-armour-cost-root {
		width: 100%;
		height: 100%;
		background: transparent;
	}
</style>
