<script lang="ts">
	import { Badge, Button, Divider, ErrorNotice, Input, Modal, SegmentedControl, Select } from '$lib/components';
	import { InDevelopmentMark } from '$lib/inDevelopment';
	import type { ProtectionSet } from '$lib/api';
	import type { ProtectionModel } from './protectionModel.svelte';
	import ProtectionObservationModal from './ProtectionObservationModal.svelte';

	let { model }: { model: ProtectionModel } = $props();

	const setSaveDisabled = $derived(
		!model.setName.trim() ||
		(model.setEconomyKind === 'limited' &&
			(!Number.isFinite(Number(model.setMarkup)) || Number(model.setMarkup) < 100)),
	);
	const loadoutSaveDisabled = $derived(
		!model.loadoutName.trim() ||
		(!model.loadoutArmourId &&
			!model.loadoutPlateId &&
			model.loadoutName.trim().toLowerCase() !== 'no protection'),
	);
	const editingSet = $derived(
		model.editingSetId
			? (model.overview.sets.find((set) => set.id === model.editingSetId) ?? null)
			: null,
	);

	function basis(set: ProtectionSet): string {
		return set.economyKind === 'limited'
			? `${set.markupPercent?.toFixed(2)}% average MU`
			: 'Raw TT repair cost';
	}

	function formatWhen(epoch: number): string {
		return new Date(epoch * 1000).toLocaleString([], {
			day: 'numeric',
			month: 'short',
			hour: '2-digit',
			minute: '2-digit',
		});
	}
</script>

<div class="space-y-7">
	<div class="flex items-start justify-between gap-6">
		<div>
			<div class="flex items-center gap-2.5">
				<h2 class="text-lg font-semibold text-text">Armour</h2>
				<InDevelopmentMark id="limited-protection" />
			</div>
			<p class="mt-1 text-sm text-text-secondary">Compose armour and plates once, then declare what is in use from the overlay.</p>
		</div>
		<div class="flex gap-2">
			<Button variant="secondary" size="sm" onclick={() => model.openSet('armour')}>Add armour set</Button>
			<Button variant="secondary" size="sm" onclick={() => model.openSet('plates')}>Add plate set</Button>
			<Button size="sm" onclick={model.openLoadout}>New loadout</Button>
		</div>
	</div>

	{#if model.error}
		<ErrorNotice message={model.error} onDismiss={() => (model.error = null)} />
	{/if}

	{#if model.loading}
		<div class="py-14 text-center text-sm text-text-tertiary animate-pulse">Loading armour...</div>
	{:else}
		<section aria-labelledby="protection-loadouts-heading">
			<div class="flex items-baseline justify-between gap-4 mb-3">
				<div>
					<h3 id="protection-loadouts-heading" class="text-sm font-semibold text-text">Loadouts</h3>
					<p class="mt-0.5 text-xs text-text-tertiary">The selected loadout becomes the next session's default.</p>
				</div>
				{#if !model.overview.loadouts.some((loadout) => !loadout.armour && !loadout.plates)}
					<button class="linklet" type="button" onclick={model.makeNoProtection}>Add no-protection option</button>
				{/if}
			</div>

			{#if model.overview.loadouts.length === 0}
				<div class="border-y border-border/70 py-8 text-center">
					<p class="text-sm font-medium text-text">No armour loadouts yet</p>
					<p class="mt-1 text-xs text-text-tertiary">Add armour or plates above, then combine them into the choices you use while playing.</p>
				</div>
			{:else}
				<div class="divide-y divide-border/60 border-y border-border/70">
					{#each model.overview.loadouts as loadout (loadout.id)}
						<div class="group flex items-center gap-4 py-3.5">
							<button
								type="button"
								class="h-4 w-4 rounded-full border flex items-center justify-center transition-colors {loadout.id === model.overview.activeLoadoutId ? 'border-accent' : 'border-border-bright hover:border-accent/60'}"
								aria-label={`Use ${loadout.name}`}
								aria-pressed={loadout.id === model.overview.activeLoadoutId}
								disabled={model.saving}
								onclick={() => model.selectLoadout(loadout.id)}
							>
								{#if loadout.id === model.overview.activeLoadoutId}<span class="h-2 w-2 rounded-full bg-accent"></span>{/if}
							</button>
							<div class="min-w-0 flex-1">
								<div class="flex items-center gap-2">
									<span class="text-sm font-medium text-text truncate">{loadout.name}</span>
									{#if loadout.id === model.overview.activeLoadoutId}<Badge variant="accent">Active</Badge>{/if}
								</div>
								<p class="mt-0.5 text-xs text-text-tertiary truncate">
									{loadout.armour?.name ?? 'No armour'} <span class="mx-1 text-border-bright">+</span> {loadout.plates?.name ?? 'No plates'}
								</p>
							</div>
							{#if loadout.armour}<Badge>{loadout.armour.economyKind === 'limited' ? 'Armour L' : 'Armour UL'}</Badge>{/if}
							{#if loadout.plates}<Badge>{loadout.plates.economyKind === 'limited' ? 'Plates L' : 'Plates UL'}</Badge>{/if}
							<div class="flex items-center gap-1 opacity-70 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
								<Button variant="ghost" size="sm" onclick={() => model.editLoadout(loadout.id)}>Edit</Button>
								<Button variant="ghost" size="sm" class="text-error" onclick={() => model.askRemoveLoadout(loadout.id)}>Remove</Button>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</section>

		<Divider />

		<div class="grid grid-cols-2 gap-10">
			{#each [{ title: 'Armour sets', sets: model.armourSets }, { title: 'Plate sets', sets: model.plateSets }] as group}
				<section aria-label={group.title}>
					<h3 class="text-sm font-semibold text-text mb-3">{group.title}</h3>
					{#if group.sets.length === 0}
						<p class="border-t border-border/70 py-5 text-xs text-text-tertiary">None configured.</p>
					{:else}
						<div class="divide-y divide-border/60 border-y border-border/70">
							{#each group.sets as set (set.id)}
								<div class="flex items-center gap-3 py-3">
									<div class="min-w-0 flex-1">
										<div class="flex items-center gap-2">
											<span class="text-sm font-medium text-text truncate">{set.name}</span>
											<Badge variant={set.economyKind === 'limited' ? 'accent' : 'neutral'}>{set.economyKind === 'limited' ? 'L' : 'UL'}</Badge>
											{#if set.pendingReconciliations > 0}<Badge variant="warning">{set.pendingReconciliations} pending</Badge>{/if}
										</div>
									<p class="mt-0.5 text-xs text-text-tertiary">{basis(set)}</p>
									{#if set.unsettledSessions > 0}
										<p class="mt-1 text-[10px] tabular-nums text-warning">Awaiting cost: {set.unsettledDamage.toFixed(1)} damage{set.unsettledDeflections > 0 ? ` + ${set.unsettledDeflections} deflected` : ''} across {set.unsettledSessions} {set.unsettledSessions === 1 ? 'session' : 'sessions'}</p>
									{/if}
									</div>
									{#if set.latestObservation}
										<div class="text-right shrink-0">
											<div class="text-sm tabular-nums text-text">{set.latestObservation.ttValuePed.toFixed(2)} PED</div>
											<div class="text-[10px] text-text-tertiary">{formatWhen(set.latestObservation.observedAt)}</div>
										</div>
									{/if}
									{#if set.economyKind === 'limited'}
										<Button variant="secondary" size="sm" onclick={() => model.openObservation(set)}>{set.latestObservation ? 'Measure' : 'Set baseline'}</Button>
									{:else}
										<span class="max-w-28 text-right text-[10px] leading-tight text-text-tertiary">Record repair TT from the overlay</span>
									{/if}
									<div class="flex items-center gap-1">
										<Button variant="ghost" size="sm" onclick={() => model.editSet(set)}>Edit</Button>
										<Button variant="ghost" size="sm" class="text-error" onclick={() => model.askRemoveSet(set)}>Remove</Button>
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</section>
			{/each}
		</div>

		{#if model.overview.recentCostWindows.length > 0}
			<Divider />
			<section aria-labelledby="protection-history-heading">
				<h3 id="protection-history-heading" class="text-sm font-semibold text-text mb-3">Recent armour costs</h3>
				<div class="divide-y divide-border/60 border-y border-border/70">
					{#each model.overview.recentCostWindows as window (window.id)}
						{@const set = model.overview.sets.find((candidate) => candidate.id === window.setId)}
						<div class="grid grid-cols-[minmax(0,1fr)_100px_110px_100px] items-center gap-4 py-2.5 text-xs">
							<div class="min-w-0"><span class="font-medium text-text">{set?.name ?? (window.kind === 'repair' ? 'Repair cost' : 'Archived set')}</span><div class="truncate text-text-tertiary">{window.reason ?? `Allocated across ${window.allocations.length} ${window.allocations.length === 1 ? 'session' : 'sessions'}`}</div></div>
							<div class="tabular-nums text-text-secondary">{!window.costKnown ? 'Unknown' : window.consumedTtPed === null ? 'Repair' : `${window.consumedTtPed.toFixed(4)} TT`}</div>
							<div class="tabular-nums font-medium text-text">{window.costKnown ? `${window.costPed.toFixed(4)} PED` : 'Unknown'}</div>
							<div class="text-right"><Badge variant={window.status === 'booked' ? 'positive' : 'warning'}>{window.status === 'booked' ? 'Allocated' : 'Pending'}</Badge></div>
						</div>
					{/each}
				</div>
			</section>
		{/if}
	{/if}
</div>

<Modal bind:open={model.setModalOpen} title={`${model.editingSetId ? 'Edit' : 'Add'} ${model.setKind === 'armour' ? 'armour' : 'plate'} set`}>
	<div class="space-y-5">
		<div><label for="protection-set-name" class="block eyebrow mb-1.5">Set name</label><Input id="protection-set-name" bind:value={model.setName} placeholder={model.setKind === 'armour' ? 'Viceroy' : '5B plates'} /></div>
		<div>
			<span class="block eyebrow mb-1.5">Item type</span>
			<SegmentedControl options={[{ id: 'limited', label: 'Limited', disabled: editingSet?.basisLocked === true }, { id: 'unlimited', label: 'Unlimited', disabled: editingSet?.basisLocked === true }]} active={model.setEconomyKind} onchange={(id) => (model.setEconomyKind = id as 'limited' | 'unlimited')} />
			{#if editingSet?.basisLocked}<p class="mt-1.5 text-xs text-text-tertiary">The economic basis is fixed after the set's first observation or recorded use.</p>{/if}
		</div>
		{#if model.setEconomyKind === 'limited'}
			<div><label for="protection-set-markup" class="block eyebrow mb-1.5">Average acquisition markup</label><div class="flex items-center gap-2"><Input id="protection-set-markup" bind:value={model.setMarkup} type="number" min={100} step="0.01" disabled={editingSet?.basisLocked === true} class="max-w-32" /><span class="text-sm text-text-tertiary">%</span></div><p class="mt-1.5 text-xs text-text-tertiary">Use the TT-weighted average paid across all seven pieces. This is intentionally approximate.</p></div>
		{:else}
			<p class="border-l-2 border-border-bright pl-3 text-xs text-text-secondary">Unlimited armour is costed from raw TT repair cost. Purchase markup remains durable capital.</p>
		{/if}
		<div class="flex justify-end gap-2"><Button variant="secondary" onclick={() => (model.setModalOpen = false)}>Cancel</Button><Button onclick={model.saveSet} disabled={setSaveDisabled} loading={model.saving}>{model.editingSetId ? 'Save changes' : 'Add set'}</Button></div>
	</div>
</Modal>

<Modal bind:open={model.loadoutModalOpen} title={model.editingLoadoutId ? 'Edit armour loadout' : 'New armour loadout'}>
	<div class="space-y-5">
		<div><label for="protection-loadout-name" class="block eyebrow mb-1.5">Loadout name</label><Input id="protection-loadout-name" bind:value={model.loadoutName} placeholder="L armour + 5B" /></div>
		<div class="grid grid-cols-2 gap-4">
			<div><label for="protection-loadout-armour" class="block eyebrow mb-1.5">Armour</label><Select id="protection-loadout-armour" bind:value={model.loadoutArmourId}><option value="">None</option>{#each model.armourSets as set}<option value={set.id}>{set.name} ({set.economyKind === 'limited' ? 'L' : 'UL'})</option>{/each}</Select></div>
			<div><label for="protection-loadout-plates" class="block eyebrow mb-1.5">Plates</label><Select id="protection-loadout-plates" bind:value={model.loadoutPlateId}><option value="">None</option>{#each model.plateSets as set}<option value={set.id}>{set.name} ({set.economyKind === 'limited' ? 'L' : 'UL'})</option>{/each}</Select></div>
		</div>
		{#if !model.loadoutArmourId && !model.loadoutPlateId}<p class="text-xs text-text-tertiary">An empty loadout is explicit evidence of unprotected play and must be named “No protection”.</p>{/if}
		<div class="flex justify-end gap-2"><Button variant="secondary" onclick={() => (model.loadoutModalOpen = false)}>Cancel</Button><Button onclick={model.saveLoadout} disabled={loadoutSaveDisabled} loading={model.saving}>{model.editingLoadoutId ? 'Save changes' : 'Create loadout'}</Button></div>
	</div>
</Modal>

<Modal bind:open={model.removalModalOpen} title={`Remove ${model.removalTarget?.kind === 'set' ? 'armour set' : 'loadout'}`}>
	{#if model.removalTarget}
		<div class="space-y-5">
			<p class="text-sm text-text-secondary">
				Remove <span class="font-medium text-text">{model.removalTarget.name}</span> from the available armour choices? Recorded sessions and measurements keep their historical identity.
			</p>
			{#if model.removalTarget.kind === 'set'}
				<p class="border-l-2 border-warning pl-3 text-xs text-text-secondary">A set can only be removed after every loadout using it has been edited or removed.</p>
			{/if}
			<div class="flex justify-end gap-2">
				<Button variant="secondary" onclick={() => (model.removalModalOpen = false)} disabled={model.saving}>Cancel</Button>
				<Button variant="danger" onclick={model.confirmRemoval} loading={model.saving}>Remove</Button>
			</div>
		</div>
	{/if}
</Modal>

<ProtectionObservationModal {model} />
