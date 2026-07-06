<script lang="ts">
	import type { Equipment } from '$lib/types';
	import type { Hotbar } from '$lib/types/settings';
	import { updateSettings } from '$lib/api';
	import { Select } from '$lib/components';

	let {
		equipment,
		hotbar: initialHotbar,
		enabled = true,
		onchange
	}: {
		equipment: Equipment[];
		hotbar: Hotbar;
		enabled?: boolean;
		onchange?: (value: Hotbar) => void;
	} = $props();

	let hotbar: Hotbar = $state({});
	let error: string | null = $state(null);

	let weapons = $derived(equipment.filter((e) => e.type === 'weapon'));
	let healingTools = $derived(equipment.filter((e) => e.type === 'healing'));
	let consumablesList = $derived(equipment.filter((e) => e.type === 'consumable'));

	$effect(() => {
		hotbar = { ...initialHotbar };
	});

	function slotItem(slot: string): Equipment | null {
		const id = hotbar[slot];
		if (id === null || id === undefined) return null;
		return equipment.find((e) => String(e.id) === String(id)) ?? null;
	}

	function slotName(slot: string): string | null {
		return slotItem(slot)?.name ?? null;
	}

	function slotCost(slot: string): number | null {
		return slotItem(slot)?.costPerUse ?? null;
	}

	function slotType(slot: string): 'weapon' | 'healing' | 'consumable' | null {
		return slotItem(slot)?.type ?? null;
	}

	async function assignSlot(slot: string, equipId: string | null) {
		if (!enabled) return;
		error = null;
		const previous = { ...hotbar };
		const nextHotbar = {
			...hotbar,
			[slot]: equipId ? parseInt(equipId, 10) : null
		};
		hotbar = nextHotbar;

		try {
			const updated = await updateSettings({ hotbar: nextHotbar });
			hotbar = { ...updated.hotbar };
			onchange?.(hotbar);
		} catch (e) {
			hotbar = previous;
			error = e instanceof Error ? e.message : 'Failed to save hotbar';
		}
	}
</script>

<div class="space-y-4">
	{#if error}
		<div class="rounded-md border border-error/20 bg-error/10 px-3 py-2">
			<p class="text-sm text-error">{error}</p>
		</div>
	{/if}

	<div class="relative min-h-64">
		{#if !enabled}
			<div class="absolute inset-0 z-10 flex items-center justify-center px-4 py-10">
				<div
					data-guide-anchor="hotbar-disabled-notice"
					class="max-w-md rounded-md border border-border bg-surface-raised/95 px-5 py-4 text-center shadow-lg backdrop-blur-sm"
				>
					<p class="text-sm font-medium text-text">Trifecta is currently in use.</p>
					<p class="mt-1.5 text-sm leading-6 text-text-secondary">
						To use the Hotbar, enable the hotbar key listener in the
						<a
							href="/settings#cost-attribution"
							class="whitespace-nowrap font-medium text-accent underline underline-offset-4 hover:text-accent-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
						>
							Settings tab
						</a>.
					</p>
				</div>
			</div>
		{/if}

		<div class="{enabled ? '' : 'opacity-40 grayscale blur-[2px] pointer-events-none select-none'}">
		{#if weapons.length === 0 && healingTools.length === 0 && consumablesList.length === 0}
			<p class="text-sm text-text-tertiary py-4">
				Add equipment in the Library tab first, then assign them to hotbar slots here.
			</p>
		{:else}
			<div class="space-y-1" data-guide-anchor="hotbar-slot-list">
				{#each ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'] as slot}
					{@const name = slotName(slot)}
					{@const cost = slotCost(slot)}
					{@const type = slotType(slot)}
					<div
						class="flex items-center gap-3 px-3 py-2.5 rounded-md
							{name ? 'bg-surface-hover/30' : 'hover:bg-surface-hover/20'}
							transition-colors duration-[var(--duration-fast)]"
					>
						<!-- Slot key badge -->
						<div
							class="shrink-0 h-7 w-7 rounded-md flex items-center justify-center text-xs font-bold
								{name
								? type === 'healing'
									? 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/30'
									: type === 'consumable'
										? 'bg-amber-500/15 text-amber-400 border border-amber-500/30'
										: 'bg-accent/15 text-accent border border-accent/30'
								: 'bg-surface text-text-tertiary border border-border'}"
						>
							{slot}
						</div>

						<!-- Equipment selector -->
						<div class="flex-1 min-w-0">
							<Select
								value={hotbar[slot] != null ? String(hotbar[slot]) : ''}
								onchange={(e) => assignSlot(slot, e.currentTarget.value || null)}
								disabled={!enabled}
							>
								<option value="">— Empty slot —</option>
								{#if weapons.length > 0}
									<optgroup label="Weapons">
										{#each weapons as w}
											<option value={w.id}>
												{w.name}
												{#if w.amplifierName}+ {w.amplifierName}{/if}
											</option>
										{/each}
									</optgroup>
								{/if}
								{#if healingTools.length > 0}
									<optgroup label="Healing Tools">
										{#each healingTools as h}
											<option value={h.id}>{h.name}</option>
										{/each}
									</optgroup>
								{/if}
								{#if consumablesList.length > 0}
									<optgroup label="Consumables">
										{#each consumablesList as c}
											<option value={c.id}>{c.name}</option>
										{/each}
									</optgroup>
								{/if}
							</Select>
						</div>

						<!-- Cost display -->
						<div class="shrink-0 w-20 text-right">
							{#if type === 'consumable'}
								<span class="text-xs text-text-tertiary">consumable</span>
							{:else if cost !== null}
								<span class="text-sm font-medium tabular-nums text-text">
									{cost.toFixed(2)}
								</span>
								<span class="text-xs text-text-tertiary ml-0.5">PEC</span>
							{:else}
								<span class="text-xs text-text-tertiary">—</span>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		{/if}
		</div>
	</div>
</div>
