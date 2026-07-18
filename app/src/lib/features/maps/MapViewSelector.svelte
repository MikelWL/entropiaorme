<script lang="ts">
	import { tick } from 'svelte';
	import Menu from '$lib/components/Menu.svelte';
	import type { MapView } from '$lib/api';

	let {
		views,
		selectedId,
		onselect,
		onadd,
		onrename,
		ondelete,
	}: {
		views: MapView[];
		selectedId: number | null;
		onselect: (id: number | null) => void;
		onadd: () => Promise<MapView | null>;
		onrename: (id: number, name: string) => Promise<boolean>;
		ondelete: (view: MapView) => Promise<boolean>;
	} = $props();

	const selectedName = $derived(views.find((view) => view.id === selectedId)?.name ?? 'Default');
	let editingId = $state<number | null>(null);
	let draftName = $state('');
	let editInput = $state<HTMLInputElement | null>(null);
	let saving = $state(false);
	let rowAction = false;

	async function addView() {
		const created = await onadd();
		if (!created) return;
		editingId = created.id;
		draftName = created.name;
		await tick();
		editInput?.focus();
		editInput?.select();
	}

	async function saveName(view: MapView) {
		const name = draftName.trim();
		if (saving) return;
		if (!name || name === view.name) {
			draftName = view.name;
			editingId = null;
			return;
		}
		saving = true;
		try {
			if (await onrename(view.id, name)) editingId = null;
		} finally {
			saving = false;
		}
	}

	async function deleteView(view: MapView) {
		try {
			if (!window.confirm(`Delete “${view.name}” and all pins on it?`)) return;
			if (await ondelete(view)) editingId = null;
		} finally {
			rowAction = false;
		}
	}
</script>

<Menu ariaLabel="Map view" class="w-full" panelClass="left-0 right-auto top-10 w-64 p-1">
	{#snippet trigger({ open, toggle })}
		<button
			type="button"
			class="flex h-9 w-full cursor-pointer items-center justify-between rounded-md border border-border bg-surface/70 pl-3 pr-2 text-left text-sm text-text transition-colors hover:border-border-bright focus:outline-none focus:border-accent/60"
			aria-haspopup="menu"
			aria-expanded={open}
			onclick={toggle}
		>
			<span class="truncate">{selectedName}</span>
			<span class="text-text-tertiary" aria-hidden="true">⌄</span>
		</button>
	{/snippet}

	{#snippet children({ close })}
		<button
			type="button"
			role="menuitem"
			class="flex w-full items-center rounded px-2 py-1.5 text-left text-xs {selectedId === null ? 'bg-accent/10 text-accent' : 'text-text-secondary hover:bg-surface-hover hover:text-text'}"
			onclick={() => {
				onselect(null);
				close();
			}}
		>
			Default
		</button>

		{#each views as view (view.id)}
			<div class="mt-0.5 flex items-center gap-1 rounded {selectedId === view.id ? 'bg-accent/10' : ''}">
				{#if editingId === view.id}
					<form class="min-w-0 flex-1" onsubmit={(event) => {
						event.preventDefault();
						void saveName(view);
					}}>
						<input
							bind:this={editInput}
							bind:value={draftName}
							aria-label="Map name"
							maxlength="40"
							class="h-7 w-full rounded border border-accent/50 bg-base px-2 text-xs text-text focus:outline-none"
							onblur={() => {
								if (rowAction) {
									draftName = view.name;
									rowAction = false;
									return;
								}
								void saveName(view);
							}}
						/>
					</form>
				{:else}
					<button
						type="button"
						role="menuitem"
						class="min-w-0 flex-1 truncate px-2 py-1.5 text-left text-xs {selectedId === view.id ? 'text-accent' : 'text-text-secondary hover:text-text'}"
						onclick={() => {
							onselect(view.id);
							close();
						}}
					>
						{view.name}
					</button>
				{/if}
				<button
					type="button"
					role="menuitem"
					aria-label="Rename {view.name}"
					class="h-7 w-7 rounded text-xs text-text-tertiary hover:bg-surface-hover hover:text-text"
					onpointerdown={() => (rowAction = true)}
					onclick={() => {
						rowAction = false;
						editingId = view.id;
						draftName = view.name;
						void tick().then(() => {
							editInput?.focus();
							editInput?.select();
						});
					}}
				>✎</button>
				<button
					type="button"
					role="menuitem"
					aria-label="Delete {view.name}"
					class="h-7 w-7 rounded text-xs text-text-tertiary hover:bg-error/10 hover:text-error"
					onpointerdown={() => (rowAction = true)}
					onclick={() => void deleteView(view)}
				>×</button>
			</div>
		{/each}

		<button
			type="button"
			role="menuitem"
			class="mt-1 flex w-full items-center rounded border-t border-border/50 px-2 py-2 text-left text-xs text-text-secondary hover:bg-surface-hover hover:text-text"
			onclick={() => void addView()}
		>
			<span class="mr-2 text-base leading-none" aria-hidden="true">+</span>
			Add map
		</button>
	{/snippet}
</Menu>
