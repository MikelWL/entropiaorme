<script lang="ts">
	import type { Equipment } from '$lib/types';
	import type { HarvestGuardrailSettings } from '$lib/types/settings';
	import { updateSettings } from '$lib/api';
	import { Select, Toggle } from '$lib/components';

	let {
		equipment,
		guardrail: initialGuardrail,
		onchange
	}: {
		equipment: Equipment[];
		guardrail: HarvestGuardrailSettings;
		onchange?: (value: HarvestGuardrailSettings) => void;
	} = $props();

	let guardrail: HarvestGuardrailSettings = $state({
		enabled: false,
		shortToolId: null,
		longToolId: null,
		hugeToolId: null
	});
	let error: string | null = $state(null);

	let harvestingTools = $derived(equipment.filter((e) => e.type === 'tool'));

	$effect(() => {
		guardrail = { ...initialGuardrail };
	});

	const treeSizes: {
		key: 'shortToolId' | 'longToolId' | 'hugeToolId';
		label: string;
		hint: string;
	}[] = [
		{ key: 'shortToolId', label: 'Short trees', hint: 'drop Short boards' },
		{ key: 'longToolId', label: 'Long trees', hint: 'drop plain boards' },
		{ key: 'hugeToolId', label: 'Huge trees', hint: 'drop Long boards' }
	];

	function toolFor(id: number | null): Equipment | null {
		if (id === null) return null;
		return harvestingTools.find((e) => String(e.id) === String(id)) ?? null;
	}

	async function persist(next: HarvestGuardrailSettings) {
		error = null;
		const previous = { ...guardrail };
		guardrail = next;
		try {
			const updated = await updateSettings({
				harvest_guardrail: {
					enabled: next.enabled,
					short_tool_id: next.shortToolId,
					long_tool_id: next.longToolId,
					huge_tool_id: next.hugeToolId
				}
			});
			guardrail = updated.harvestGuardrail;
			onchange?.(guardrail);
		} catch (e) {
			guardrail = previous;
			error = e instanceof Error ? e.message : 'Failed to save guardrail';
		}
	}

	function assignTool(key: 'shortToolId' | 'longToolId' | 'hugeToolId', value: string | null) {
		void persist({ ...guardrail, [key]: value ? parseInt(value, 10) : null });
	}
</script>

<div class="space-y-4">
	{#if error}
		<div class="rounded-md border border-error/20 bg-error/10 px-3 py-2">
			<p class="text-sm text-error">{error}</p>
		</div>
	{/if}

	<div class="flex items-start justify-between gap-4">
		<div>
			<h2 class="text-sm font-medium text-text">Tree-cutting tool guardrail</h2>
			<p class="mt-1 text-sm leading-6 text-text-secondary max-w-xl">
				Declare the tool you intend to use on each tree size. While tracking, the board type
				in each loot names the tree, so costs are attributed to the intended tool even if the
				hotbar missed a switch, and the overlay flags the disagreement in red.
			</p>
		</div>
		<Toggle
			checked={guardrail.enabled}
			label="Enable harvest guardrail"
			onchange={(checked) => void persist({ ...guardrail, enabled: checked })}
		/>
	</div>

	<div class={guardrail.enabled ? '' : 'opacity-40 pointer-events-none select-none'}>
		{#if harvestingTools.length === 0}
			<p class="text-sm text-text-tertiary py-4">
				Add harvesting tools in the Library tab first, then assign one per tree size here.
			</p>
		{:else}
			<div class="space-y-1">
				{#each treeSizes as size}
					{@const tool = toolFor(guardrail[size.key])}
					<div
						class="flex items-center gap-3 px-3 py-2.5 rounded-md
							{tool ? 'bg-surface-hover/30' : 'hover:bg-surface-hover/20'}
							transition-colors duration-[var(--duration-fast)]"
					>
						<div class="shrink-0 w-28">
							<span class="text-sm font-medium text-text">{size.label}</span>
							<span class="block text-xs text-text-tertiary">{size.hint}</span>
						</div>
						<div class="flex-1 min-w-0">
							<Select
								value={guardrail[size.key] != null ? String(guardrail[size.key]) : ''}
								onchange={(e) => assignTool(size.key, e.currentTarget.value || null)}
								disabled={!guardrail.enabled}
							>
								<option value="">– No intended tool –</option>
								{#each harvestingTools as t}
									<option value={t.id}>{t.name}</option>
								{/each}
							</Select>
						</div>
						<div class="shrink-0 w-20 text-right">
							{#if tool?.costPerUse != null}
								<span class="text-sm font-medium tabular-nums text-text">
									{tool.costPerUse.toFixed(2)}
								</span>
								<span class="text-xs text-text-tertiary ml-0.5">PEC</span>
							{:else}
								<span class="text-xs text-text-tertiary">–</span>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
