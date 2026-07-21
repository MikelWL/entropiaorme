<script lang="ts">
	import Button from '$lib/components/Button.svelte';

	let {
		mode,
		count,
		treeCount = 0,
		onclear,
		oncancel,
		onconfirm = () => {},
		ondelete = () => {},
		oncooldown = () => {},
	}: {
		mode: 'route' | 'pins';
		count: number;
		treeCount?: number;
		onclear: () => void;
		oncancel: () => void;
		onconfirm?: () => void;
		ondelete?: () => void;
		oncooldown?: () => void;
	} = $props();
</script>

<div
	class="absolute left-1/2 top-2 z-20 w-[min(40rem,calc(100%-1rem))] -translate-x-1/2 rounded-lg border border-accent/40 bg-surface-raised/95 p-3 shadow-xl backdrop-blur"
	role="dialog"
	aria-label={mode === 'route' ? 'Choose route area' : 'Select map pins'}
	tabindex="-1"
	onpointerdown={(event) => event.stopPropagation()}
>
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="min-w-0">
			<p class="text-sm font-semibold text-text">
				{mode === 'route' ? 'Choose route area' : 'Select map pins'}
			</p>
			<p class="mt-0.5 text-xs text-text-secondary">
				Drag to add areas. Remove an area with its × button. Hold Space to pan.
			</p>
			<p class="mt-1 text-xs font-medium text-accent" aria-live="polite">
				{#if mode === 'route'}
					{count === 1 ? '1 eligible tree selected' : `${count} eligible trees selected`}
				{:else}
					{count === 1 ? '1 pin selected' : `${count} pins selected`}
				{/if}
			</p>
		</div>
		<div class="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
			<Button size="sm" variant="ghost" onclick={onclear}>Clear</Button>
			<Button size="sm" variant="secondary" onclick={oncancel}>Cancel</Button>
			{#if mode === 'route'}
				<Button size="sm" disabled={count === 0} onclick={onconfirm}>
					{count === 1 ? 'Use 1 tree' : `Use ${count} trees`}
				</Button>
			{:else}
				<Button size="sm" variant="secondary" disabled={treeCount === 0} onclick={oncooldown}>
					{treeCount === 1 ? 'Cooldown 1 tree' : `Cooldown ${treeCount} trees`}
				</Button>
				<Button size="sm" variant="danger" disabled={count === 0} onclick={ondelete}>
					{count === 1 ? 'Delete 1 pin' : `Delete ${count} pins`}
				</Button>
			{/if}
		</div>
	</div>
</div>
