<script lang="ts">
	let { children } = $props();
</script>

<div class="overlay-root select-none">
	{@render children()}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		overflow: clip !important;
		margin: 0 !important;
		padding: 0 !important;
		width: max-content !important;
		height: max-content !important;
		min-width: 0 !important;
		min-height: 0 !important;
	}

	.overlay-root {
		background: transparent;
		overflow: visible;
		width: max-content;
		height: max-content;
	}
</style>
