<script lang="ts">
	let {
		message,
		onDismiss,
		class: className = ''
	}: {
		/** Error text to surface; nothing renders when null. */
		message: string | null;
		/** When provided, a linklet-styled Dismiss control is rendered. */
		onDismiss?: () => void;
		class?: string;
	} = $props();
</script>

{#if message}
	{#if onDismiss}
		<div
			role="alert"
			class="text-sm text-negative bg-negative/10 rounded-md px-3 py-2 flex items-center justify-between gap-3 {className}"
		>
			<span>{message}</span>
			<button type="button" class="linklet" onclick={onDismiss}>Dismiss</button>
		</div>
	{:else}
		<div role="alert" class="text-sm text-negative bg-negative/10 rounded-md px-3 py-2 {className}">
			{message}
		</div>
	{/if}
{/if}
