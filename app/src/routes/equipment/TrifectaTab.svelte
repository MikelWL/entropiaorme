<script lang="ts">
	import { updateSettings } from '$lib/api';
	import { Button, Input, Select } from '$lib/components';
	import type { Equipment } from '$lib/types';
	import type { TrifectaPreset, TrifectaSettings } from '$lib/types/settings';

	let {
		equipment,
		trifecta: initialTrifecta,
		enabled = true,
		onchange
	}: {
		equipment: Equipment[];
		trifecta: TrifectaSettings;
		enabled?: boolean;
		onchange?: (value: TrifectaSettings) => void;
	} = $props();

	type Interval = { min: number; max: number };
	type WeaponBand = {
		name: string;
		tone: 'small' | 'big';
		normal: Interval;
		crit: Interval;
	};

	let trifecta: TrifectaSettings = $state(emptyTrifecta());
	let error: string | null = $state(null);
	let hoverTip = $state<{ text: string; x: number; y: number } | null>(null);
	let presetNameDraft = $state('');
	let newPresetName = $state('');
	let toolbarMode = $state<'idle' | 'rename' | 'create' | 'delete'>('idle');

	let weapons = $derived(equipment.filter((e) => e.type === 'weapon'));
	let healingTools = $derived(equipment.filter((e) => e.type === 'healing'));
	let activePreset = $derived.by(
		() => trifecta.presets.find((preset) => preset.id === trifecta.activePresetId) ?? trifecta.presets[0] ?? null
	);
	let smallWeapon = $derived(getItem(activePreset?.smallWeaponId ?? null));
	let bigWeapon = $derived(getItem(activePreset?.bigWeaponId ?? null));

	let weaponBands = $derived.by(() => {
		const bands: WeaponBand[] = [];
		for (const [item, tone] of [
			[smallWeapon, 'small'],
			[bigWeapon, 'big']
		] as const) {
			if (!item || item.damageMin === null || item.damageMax === null) continue;
			const normal = { min: item.damageMin, max: item.damageMax };
			bands.push({
				name: item.name,
				tone,
				normal,
				crit: {
					min: normal.min * 2,
					max: normal.max * 3
				}
			});
		}
		return bands;
	});

	let chartMax = $derived.by(() => {
		const maxima = weaponBands.map((band) => band.crit.max);
		return maxima.length > 0 ? Math.max(...maxima) : 1;
	});

	let normalOverlapWarning = $derived.by(() => {
		const small = weaponBands.find((band) => band.tone === 'small');
		const big = weaponBands.find((band) => band.tone === 'big');
		if (!small || !big) return null;

		const overlapMin = Math.max(small.normal.min, big.normal.min);
		const overlapMax = Math.min(small.normal.max, big.normal.max);
		if (overlapMax < overlapMin) return null;

		return {
			level: 'severe',
			message: 'Hit ranges overlap: trifecta attribution unavailable',
			range: `${overlapMin.toFixed(1)} - ${overlapMax.toFixed(1)}`
		} as const;
	});

	let critOverlapWarning = $derived.by(() => {
		const small = weaponBands.find((band) => band.tone === 'small');
		const big = weaponBands.find((band) => band.tone === 'big');
		if (!small || !big) return null;

		const overlapMin = Math.max(small.crit.min, big.normal.min);
		const overlapMax = Math.min(small.crit.max, big.normal.max);
		if (overlapMax < overlapMin) return null;

		const totalOverlap =
			(small.crit.min <= big.normal.min && small.crit.max >= big.normal.max) ||
			(big.normal.min <= small.crit.min && big.normal.max >= small.crit.max);

		return {
			level: totalOverlap ? 'severe' : 'warning',
			message: totalOverlap ? 'Small crit fully covers big hit' : 'Small crit overlaps big hit',
			range: `${overlapMin.toFixed(1)} - ${overlapMax.toFixed(1)}`
		} as const;
	});

	$effect(() => {
		trifecta = cloneTrifecta(initialTrifecta);
	});

	$effect(() => {
		presetNameDraft = activePreset?.name ?? '';
	});

	function cloneTrifecta(value: TrifectaSettings): TrifectaSettings {
		return {
			...value,
			presets: value.presets.map((preset) => ({ ...preset }))
		};
	}

	function emptyTrifecta(): TrifectaSettings {
		return {
			activePresetId: null,
			activePresetName: null,
			presets: [],
			ready: false,
			message: null
		};
	}

	function getItem(id: number | null): Equipment | null {
		if (id === null) return null;
		return equipment.find((item) => Number(item.id) === id) ?? null;
	}

	function formatRange(interval: Interval): string {
		return `${interval.min.toFixed(1)} - ${interval.max.toFixed(1)}`;
	}

	function intervalStyle(interval: Interval): string {
		const left = (interval.min / chartMax) * 100;
		const width = Math.max(((interval.max - interval.min) / chartMax) * 100, 2);
		return `left: ${left}%; width: ${width}%;`;
	}

	function showTip(event: MouseEvent, text: string) {
		const rect = (event.currentTarget as HTMLElement).closest('[data-dashboard-root]')?.getBoundingClientRect();
		if (!rect) return;
		hoverTip = {
			text,
			x: event.clientX - rect.left,
			y: event.clientY - rect.top
		};
	}

	function moveTip(event: MouseEvent, text: string) {
		showTip(event, text);
	}

	function hideTip() {
		hoverTip = null;
	}

	function toUpdatePayload(presets: TrifectaPreset[]) {
		return presets.map((preset) => ({
			id: preset.id,
			name: preset.name,
			small_weapon_id: preset.smallWeaponId,
			big_weapon_id: preset.bigWeaponId,
			heal_id: preset.healId
		}));
	}

	async function persist(next: TrifectaSettings) {
		if (!enabled) return;
		error = null;
		const previous = cloneTrifecta(trifecta);
		trifecta = cloneTrifecta(next);

		try {
			const updated = await updateSettings({
				active_trifecta_preset_id: next.activePresetId,
				trifecta_presets: toUpdatePayload(next.presets)
			});
			trifecta = cloneTrifecta(updated.trifecta);
			onchange?.(trifecta);
		} catch (e) {
			trifecta = previous;
			error = e instanceof Error ? e.message : 'Failed to save trifecta preset';
		}
	}

	async function selectPreset(presetId: string) {
		if (!enabled) return;
		await persist({
			...trifecta,
			activePresetId: presetId
		});
	}

	async function createPreset() {
		if (!enabled) return;
		const name = newPresetName.trim();
		if (!name) {
			error = 'Preset name is required';
			return;
		}

		const source = activePreset;
		const presetId = globalThis.crypto?.randomUUID?.() ?? `preset-${Date.now()}`;
		const nextPreset: TrifectaPreset = {
			id: presetId,
			name,
			smallWeaponId: source?.smallWeaponId ?? null,
			bigWeaponId: source?.bigWeaponId ?? null,
			healId: source?.healId ?? null,
			ready: false,
			message: null
		};

		newPresetName = '';
		toolbarMode = 'idle';
		await persist({
			...trifecta,
			activePresetId: presetId,
			presets: [...trifecta.presets, nextPreset]
		});
	}

	function openCreatePreset() {
		if (!enabled) return;
		error = null;
		newPresetName = '';
		toolbarMode = 'create';
	}

	function openRenamePreset() {
		if (!enabled) return;
		if (!activePreset) return;
		error = null;
		presetNameDraft = activePreset.name;
		toolbarMode = 'rename';
	}

	function cancelToolbarMode() {
		if (!enabled) return;
		newPresetName = '';
		toolbarMode = 'idle';
	}

	function openDeletePreset() {
		if (!enabled) return;
		error = null;
		toolbarMode = 'delete';
	}

	async function deletePreset() {
		if (!enabled) return;
		if (!activePreset || trifecta.presets.length <= 1) return;

		const currentIndex = trifecta.presets.findIndex((preset) => preset.id === activePreset.id);
		const nextPresets = trifecta.presets.filter((preset) => preset.id !== activePreset.id);
		const fallbackPreset =
			nextPresets[Math.min(currentIndex, nextPresets.length - 1)] ?? nextPresets[0] ?? null;

		toolbarMode = 'idle';
		await persist({
			...trifecta,
			activePresetId: fallbackPreset?.id ?? null,
			presets: nextPresets
		});
	}

	async function savePresetName() {
		if (!enabled) return;
		if (!activePreset || toolbarMode !== 'rename') return;
		const name = presetNameDraft.trim();
		if (!name) {
			error = 'Preset name is required';
			presetNameDraft = activePreset.name;
			return;
		}
		if (name === activePreset.name) {
			toolbarMode = 'idle';
			return;
		}

		await persist({
			...trifecta,
			presets: trifecta.presets.map((preset) =>
				preset.id === activePreset.id ? { ...preset, name } : preset
			)
		});
		toolbarMode = 'idle';
	}

	async function assign(field: 'smallWeaponId' | 'bigWeaponId' | 'healId', value: string) {
		if (!enabled) return;
		if (!activePreset) return;
		const parsed = value ? parseInt(value, 10) : null;
		await persist({
			...trifecta,
			presets: trifecta.presets.map((preset) =>
				preset.id === activePreset.id ? { ...preset, [field]: parsed } : preset
			)
		});
	}
</script>

<div class="space-y-6 pt-2">
	{#if error}
		<div class="rounded-md border border-error/20 bg-error/10 px-3 py-2">
			<p class="text-sm text-error">{error}</p>
		</div>
	{/if}

	<div class="relative min-h-80">
		{#if !enabled}
			<div class="absolute inset-0 z-10 flex items-center justify-center px-4 py-10">
				<div class="max-w-md rounded-md border border-border bg-surface-raised/95 px-5 py-4 text-center shadow-lg backdrop-blur-sm">
					<p class="text-sm font-medium text-text">Hotbar is currently in use.</p>
					<p class="mt-1.5 text-sm leading-6 text-text-secondary">
						To use the Trifecta, disable the hotbar key listener in the
						<a
							href="/settings#cost-attribution"
							class="font-medium text-accent underline underline-offset-4 hover:text-accent-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
						>
							Settings tab
						</a>.
					</p>
				</div>
			</div>
		{/if}

	<div class="space-y-6 {enabled ? '' : 'opacity-40 grayscale blur-[2px] pointer-events-none select-none'}">
	<section class="space-y-3">
		<p class="eyebrow">Preset</p>
		<div class="overflow-x-auto">
			<div class="flex min-w-max items-center gap-2">
				{#if toolbarMode === 'rename'}
					<Input
						id="trifecta-active-preset-name"
						aria-label="Active preset name"
						class="w-44 shrink-0"
						bind:value={presetNameDraft}
						placeholder="Preset name"
						disabled={!enabled || !activePreset}
						onblur={savePresetName}
						onkeydown={(event) => {
							if (event.key === 'Enter') {
								event.preventDefault();
								savePresetName();
							}
							if (event.key === 'Escape') {
								event.preventDefault();
								cancelToolbarMode();
							}
						}}
					/>
				{:else}
					<Select
						id="trifecta-active-preset"
						class="w-44 shrink-0"
						value={trifecta.activePresetId ?? ''}
						onchange={(event) => selectPreset(event.currentTarget.value)}
						disabled={!enabled}
					>
						{#snippet children()}
							{#each trifecta.presets as preset}
								<option value={preset.id}>{preset.name}</option>
							{/each}
						{/snippet}
					</Select>
				{/if}

				{#if activePreset && !activePreset.ready}
					<div
						class="shrink-0 rounded-full border border-warning/25 bg-warning/10 px-2.5 py-1 text-[11px] font-medium text-warning"
						title={activePreset.message ?? 'Preset is not ready for trifecta attribution'}
					>
						Invalid
					</div>
				{/if}

				{#if toolbarMode === 'create'}
					<Input
						id="trifecta-create-preset"
						aria-label="New preset name"
						class="w-36 shrink-0"
						bind:value={newPresetName}
						placeholder="New preset"
						disabled={!enabled}
						onkeydown={(event) => {
							if (event.key === 'Enter') {
								event.preventDefault();
								createPreset();
							}
							if (event.key === 'Escape') {
								event.preventDefault();
								cancelToolbarMode();
							}
						}}
					/>
					<Button onclick={createPreset} disabled={!enabled}>
						{#snippet children()}Save{/snippet}
					</Button>
					<Button variant="ghost" onclick={cancelToolbarMode} disabled={!enabled}>
						{#snippet children()}Cancel{/snippet}
					</Button>
				{:else if toolbarMode === 'rename'}
					<Button variant="ghost" onclick={cancelToolbarMode} disabled={!enabled}>
						{#snippet children()}Cancel{/snippet}
					</Button>
				{:else if toolbarMode === 'delete'}
					<Button variant="secondary" disabled={!enabled || !activePreset} onclick={openRenamePreset}>
						{#snippet children()}Rename{/snippet}
					</Button>
					<Button variant="secondary" onclick={openCreatePreset} disabled={!enabled}>
						{#snippet children()}+ Preset{/snippet}
					</Button>
					<Button
						variant="danger"
						disabled={!enabled || trifecta.presets.length <= 1}
						title={trifecta.presets.length <= 1 ? 'Keep at least one preset' : 'Delete preset'}
						onclick={deletePreset}
					>
						{#snippet children()}Confirm delete{/snippet}
					</Button>
					<Button variant="ghost" onclick={cancelToolbarMode} disabled={!enabled}>
						{#snippet children()}Cancel{/snippet}
					</Button>
				{:else}
					<Button variant="secondary" disabled={!enabled || !activePreset} onclick={openRenamePreset}>
						{#snippet children()}Rename{/snippet}
					</Button>
					<Button variant="secondary" onclick={openCreatePreset} disabled={!enabled}>
						{#snippet children()}+ Preset{/snippet}
					</Button>
					<Button
						variant="secondary"
						disabled={!enabled || trifecta.presets.length <= 1}
						title={trifecta.presets.length <= 1 ? 'Keep at least one preset' : 'Delete preset'}
						onclick={openDeletePreset}
					>
						{#snippet children()}Delete{/snippet}
					</Button>
				{/if}
			</div>
		</div>
	</section>

	<section class="space-y-4">
		<div class="flex flex-wrap justify-end gap-2">
			{#if normalOverlapWarning}
				<div
					class="rounded-full bg-error/12 px-2.5 py-1 text-[11px] font-medium text-error tabular-nums"
					title={`Overlap range ${normalOverlapWarning.range}`}
				>
					{normalOverlapWarning.message}
				</div>
			{/if}
			{#if critOverlapWarning}
				<div
					class="rounded-full px-2.5 py-1 text-[11px] font-medium tabular-nums
						{critOverlapWarning.level === 'severe' ? 'bg-error/12 text-error' : 'bg-warning/12 text-warning'}"
					title={`Overlap range ${critOverlapWarning.range}`}
				>
					{critOverlapWarning.message}
				</div>
			{/if}
		</div>

		{#if weaponBands.length > 0}
			<div class="space-y-3">
				<div class="rounded-md border border-border/60 bg-base/70 px-3 py-4" data-dashboard-root data-guide-anchor="trifecta-chart">
					<div class="relative h-16">
						<div class="absolute left-0 right-0 top-1/2 h-px -translate-y-1/2 bg-border-bright"></div>
						<div class="absolute inset-y-0 left-0 right-0 bg-[linear-gradient(to_right,transparent_0%,transparent_24.5%,rgba(148,163,184,0.12)_25%,transparent_25.5%,transparent_49.5%,rgba(148,163,184,0.12)_50%,transparent_50.5%,transparent_74.5%,rgba(148,163,184,0.12)_75%,transparent_75.5%,transparent_100%)]"></div>
						{#each weaponBands as band}
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<div
								class="absolute top-1/2 h-5 -translate-y-1/2 rounded-full cursor-default
									{band.tone === 'small' ? 'bg-positive/18' : 'bg-warning/18'}"
								style={intervalStyle(band.crit)}
								onmouseenter={(event) => showTip(event, `${band.name} crit: ${formatRange(band.crit)}`)}
								onmousemove={(event) => moveTip(event, `${band.name} crit: ${formatRange(band.crit)}`)}
								onmouseleave={hideTip}
							></div>
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<div
								data-guide-anchor={band.tone === 'small' ? 'trifecta-band-small-hit' : 'trifecta-band-big-hit'}
								class="absolute top-1/2 h-3 -translate-y-1/2 rounded-full cursor-default
									{band.tone === 'small' ? 'bg-positive' : 'bg-warning'}"
								style={intervalStyle(band.normal)}
								onmouseenter={(event) => showTip(event, `${band.name} hit: ${formatRange(band.normal)}`)}
								onmousemove={(event) => moveTip(event, `${band.name} hit: ${formatRange(band.normal)}`)}
								onmouseleave={hideTip}
							></div>
						{/each}
						{#if hoverTip}
							<div
								class="pointer-events-none absolute z-10 -translate-x-1/2 -translate-y-full whitespace-nowrap rounded-md border border-border-bright bg-surface-raised px-2 py-1 text-[11px] text-text shadow-lg"
								style={`left: ${hoverTip.x}px; top: ${hoverTip.y - 8}px;`}
							>
								{hoverTip.text}
							</div>
						{/if}
					</div>
					<div class="mt-2 flex items-center justify-between text-[11px] text-text-tertiary tabular-nums">
						<span>0</span>
						<span>{(chartMax * 0.25).toFixed(0)}</span>
						<span>{(chartMax * 0.5).toFixed(0)}</span>
						<span>{(chartMax * 0.75).toFixed(0)}</span>
						<span>{chartMax.toFixed(0)}</span>
					</div>
				</div>

				<div class="flex flex-wrap gap-2 text-[11px]">
					{#each weaponBands as band}
						<div class="flex items-center gap-2 rounded-full border border-border/60 bg-base/60 px-2.5 py-1">
							<span class="h-2.5 w-2.5 rounded-full {band.tone === 'small' ? 'bg-positive' : 'bg-warning'}"></span>
							<span class="text-text-secondary">{band.name}</span>
						</div>
					{/each}
					<div class="flex items-center gap-2 rounded-full border border-border/60 bg-base/60 px-2.5 py-1">
						<span class="h-3 w-5 rounded-full bg-text-secondary/25"></span>
						<span class="text-text-secondary">Crit envelope</span>
					</div>
				</div>
			</div>
		{:else}
			<p class="text-xs text-text-tertiary">
				Select at least one weapon in the active preset to render the damage dashboard.
			</p>
		{/if}
		<div class="space-y-3 border-t border-border/60 pt-4">
			<div class="grid gap-3 md:grid-cols-3" data-guide-anchor="trifecta-selectors">
				<div class="space-y-1.5">
					<label
						class="eyebrow"
						for="trifecta-small-weapon"
					>
						Small
					</label>
					<Select
						id="trifecta-small-weapon"
						value={activePreset?.smallWeaponId != null ? String(activePreset.smallWeaponId) : ''}
						onchange={(event) => assign('smallWeaponId', event.currentTarget.value)}
						disabled={!enabled || !activePreset}
					>
						<option value="">—</option>
						{#each weapons as weapon}
							<option value={weapon.id}>{weapon.name}</option>
						{/each}
					</Select>
				</div>

				<div class="space-y-1.5">
					<label
						class="eyebrow"
						for="trifecta-big-weapon"
					>
						Big
					</label>
					<Select
						id="trifecta-big-weapon"
						value={activePreset?.bigWeaponId != null ? String(activePreset.bigWeaponId) : ''}
						onchange={(event) => assign('bigWeaponId', event.currentTarget.value)}
						disabled={!enabled || !activePreset}
					>
						<option value="">—</option>
						{#each weapons as weapon}
							<option value={weapon.id}>{weapon.name}</option>
						{/each}
					</Select>
				</div>

				<div class="space-y-1.5">
					<label
						class="eyebrow"
						for="trifecta-heal-tool"
					>
						Heal
					</label>
					<Select
						id="trifecta-heal-tool"
						value={activePreset?.healId != null ? String(activePreset.healId) : ''}
						onchange={(event) => assign('healId', event.currentTarget.value)}
						disabled={!enabled || !activePreset}
					>
						<option value="">—</option>
						{#each healingTools as tool}
							<option value={tool.id}>{tool.name}</option>
						{/each}
					</Select>
				</div>
			</div>
		</div>
	</section>
	</div>
	</div>
</div>
