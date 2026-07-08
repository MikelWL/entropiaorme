<script lang="ts">
	import { Button, Input, Modal, PickerInput, SegmentedControl } from '$lib/components';
	import { formatPec } from './display';
	import type { EquipmentFormType, LibraryModel } from './libraryModel.svelte';

	let { model }: { model: LibraryModel } = $props();

	const saveDisabled = $derived(
		(model.addType === 'weapon'
			? !model.weaponPicker.selected
			: model.addType === 'healing'
				? !model.healerPicker.selected
				: !model.consumablePicker.selected) || model.saving,
	);

	// The add-custom row appears once the query could have searched (two or
	// more characters) and no catalogue hit matches it exactly.
	const showConsumableCustomRow = $derived.by(() => {
		const trimmed = model.consumablePicker.query.trim();
		return (
			trimmed.length >= 2 &&
			!model.consumablePicker.results.some((r) => r.name.toLowerCase() === trimmed.toLowerCase())
		);
	});
</script>

{#snippet consumableCustomRow()}
	<button
		type="button"
		class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
			transition-colors duration-[var(--duration-fast)] cursor-pointer
			border-t border-border/50"
		onclick={() => model.selectConsumableCustom(model.consumablePicker.query)}
	>
		<span class="text-text-secondary">Add custom: </span>
		<span class="text-text font-medium">{model.consumablePicker.query.trim()}</span>
	</button>
{/snippet}

<!-- Add Equipment Modal -->
<Modal bind:open={model.showAddModal} title={model.editingEquipmentId ? 'Edit Equipment' : 'Add Equipment'} class="max-w-lg">
	<div class="space-y-5">
		<!-- Type toggle -->
		{#if !model.editingEquipmentId}
			<SegmentedControl
				size="md"
				options={[
					{ id: 'weapon', label: 'Weapon' },
					{ id: 'healing', label: 'Healing Tool' },
					{ id: 'consumable', label: 'Consumable' }
				]}
				active={model.addType}
				onchange={(id) => model.setAddType(id as EquipmentFormType)}
			/>
		{/if}

		{#if model.addType === 'weapon'}
			<!-- Weapon selection -->
			<div>
				<label for="equipment-weapon-search" class="block eyebrow mb-1.5">
					Weapon
				</label>
				<PickerInput id="equipment-weapon-search" model={model.weaponPicker} placeholder="Search weapons…">
					{#snippet result({ item })}
						<span class="text-text">
							{item.name}
						</span>
						<span class="text-xs text-text-tertiary tabular-nums">
							D:{item.decay.toFixed(3)} A:{item.ammoBurn.toFixed(2)} PEC
						</span>
					{/snippet}
					{#snippet selection({ item, clear })}
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{item.name}</span>
							<button type="button" class="linklet" onclick={clear}>Change</button>
						</div>
						<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
							<span>Decay: {item.decay.toFixed(3)} PEC</span>
							<span>Ammo: {item.ammoBurn.toFixed(2)} PEC/shot</span>
						</div>
					{/snippet}
				</PickerInput>
			</div>

			<!-- Amplifier (optional) -->
			<div>
				<label for="equipment-amp-search" class="block eyebrow mb-1.5">
					Amplifier <span class="font-normal text-text-tertiary">(optional)</span>
				</label>
				<PickerInput id="equipment-amp-search" model={model.ampPicker} placeholder="Search amplifiers…">
					{#snippet result({ item })}
						<span class="text-text">
							{item.name}
						</span>
						<span class="text-xs text-text-tertiary tabular-nums">
							D:{item.decay.toFixed(3)} A:{item.ammoBurn.toFixed(2)} PEC
						</span>
					{/snippet}
					{#snippet selection({ item, clear })}
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{item.name}</span>
							<button type="button" class="linklet" onclick={clear}>Remove</button>
						</div>
						<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
							<span>Decay: {item.decay.toFixed(3)} PEC</span>
							<span>Ammo: {item.ammoBurn.toFixed(2)} PEC/shot</span>
						</div>
					{/snippet}
				</PickerInput>
			</div>

			<!-- Optional attachments -->
			<div>
				<button
					type="button"
					data-guide-anchor="optional-attachments-toggle"
					class="flex items-center gap-1.5 text-xs text-text-secondary hover:text-text
						transition-colors duration-[var(--duration-fast)] cursor-pointer"
					onclick={() => (model.showOptionalAttachments = !model.showOptionalAttachments)}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor"
						class="h-3.5 w-3.5 transition-transform duration-[var(--duration-base)]
							{model.showOptionalAttachments ? 'rotate-180' : ''}">
						<path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
					</svg>
					Optional attachments (scope, absorber)
				</button>
				{#if model.showOptionalAttachments}
					<div class="mt-3 pl-4 space-y-4 border-l border-border">
						<!-- Scope -->
						<div>
							<label for="equipment-scope-search" class="block eyebrow mb-1.5">
								Scope
							</label>
							<PickerInput id="equipment-scope-search" model={model.scopePicker} placeholder="Search scopes…">
								{#snippet result({ item })}
									<span class="text-text">
										{item.name}
									</span>
									<span class="text-xs text-text-tertiary tabular-nums">
										D:{item.decay.toFixed(3)} PEC
									</span>
								{/snippet}
								{#snippet selection({ item, clear })}
									<div class="flex items-center justify-between">
										<span class="text-text font-medium">{item.name}</span>
										<button type="button" class="linklet" onclick={clear}>Remove</button>
									</div>
									<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
										<span>Decay: {item.decay.toFixed(3)} PEC</span>
									</div>
								{/snippet}
							</PickerInput>
							{#if model.scopePicker.selected?.isLimited}
								<div class="mt-1.5 flex items-center gap-2">
									<label for="equipment-scope-markup" class="text-xs text-text-tertiary">Scope markup %</label>
									<Input id="equipment-scope-markup" type="number" bind:value={model.scopeMarkupPercent} min={100} max={10000} class="w-20" />
								</div>
							{/if}
						</div>

						<!-- Absorber -->
						<div>
							<label for="equipment-absorber-search" class="block eyebrow mb-1.5">
								Absorber
							</label>
							<PickerInput id="equipment-absorber-search" model={model.absorberPicker} placeholder="Search absorbers…">
								{#snippet result({ item })}
									<span class="text-text">
										{item.name}
									</span>
								{/snippet}
								{#snippet selection({ item, clear })}
									<div class="flex items-center justify-between">
										<span class="text-text font-medium">{item.name}</span>
										<button type="button" class="linklet" onclick={clear}>Remove</button>
									</div>
								{/snippet}
							</PickerInput>
							{#if model.absorberPicker.selected?.isLimited}
								<div class="mt-1.5 flex items-center gap-2">
									<label for="equipment-absorber-markup" class="text-xs text-text-tertiary">Absorber markup %</label>
									<Input id="equipment-absorber-markup" type="number" bind:value={model.absorberMarkupPercent} min={100} max={10000} class="w-20" />
								</div>
							{/if}
						</div>
					</div>
				{/if}
			</div>

			<div>
				<label for="equipment-damage-enhancers" class="block eyebrow mb-1.5">
					Damage enhancers
				</label>
				<Input id="equipment-damage-enhancers" type="number" bind:value={model.damageEnhancers} min={0} class="w-24" />
				<p class="text-xs text-text-tertiary mt-1">
					Configured slots on this weapon. Each slot is treated as a full stack at session start.
				</p>
			</div>

			<!-- Live cost preview -->
			{#if model.liveCostPreview !== null}
				<div class="p-3 bg-accent-faint rounded-md border border-accent/20">
					<div class="flex items-center justify-between">
						<span class="eyebrow">Estimated cost per use</span>
						<span class="text-lg font-semibold tabular-nums text-accent">{formatPec(model.liveCostPreview)} PEC</span>
					</div>
				</div>
			{/if}
		{:else if model.addType === 'healing'}
			<!-- Healing tool selection -->
			<div>
				<label for="equipment-healer-search" class="block eyebrow mb-1.5">
					Healing Tool
				</label>
				<PickerInput id="equipment-healer-search" model={model.healerPicker} placeholder="Search medical tools…">
					{#snippet result({ item })}
						<span class="text-text">
							{item.name}
						</span>
						<span class="text-xs text-text-tertiary tabular-nums">
							D:{item.decay.toFixed(3)} A:{item.ammoBurn.toFixed(2)} PEC
						</span>
					{/snippet}
					{#snippet selection({ item, clear })}
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{item.name}</span>
							<button type="button" class="linklet" onclick={clear}>Change</button>
						</div>
						<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
							<span>Decay: {item.decay.toFixed(3)} PEC</span>
							<span>Ammo: {item.ammoBurn.toFixed(2)} PEC/use</span>
						</div>
					{/snippet}
				</PickerInput>
			</div>
		{:else if model.addType === 'consumable'}
			<!-- Consumable selection -->
			<div>
				<label for="equipment-consumable-search" class="block eyebrow mb-1.5">
					Consumable
				</label>
				<PickerInput
					id="equipment-consumable-search"
					model={model.consumablePicker}
					placeholder="Search or type a custom name…"
					extraRow={showConsumableCustomRow ? consumableCustomRow : undefined}
				>
					{#snippet result({ item })}
						<span class="text-text">{item.name}</span>
					{/snippet}
					{#snippet selection({ item, clear })}
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{item.name}</span>
							<button type="button" class="linklet" onclick={clear}>Change</button>
						</div>
						{#if !item.catalogId}
							<div class="mt-1 text-xs text-text-tertiary">Custom entry</div>
						{/if}
					{/snippet}
				</PickerInput>
			</div>
		{/if}

		<!-- Markup (conditional on limited items; applies to both types) -->
		{#if (model.addType === 'weapon' && (model.weaponPicker.selected?.isLimited || model.ampPicker.selected?.isLimited)) || (model.addType === 'healing' && model.healerPicker.selected?.isLimited)}
			<div>
				<label for="equipment-item-markup" class="block eyebrow mb-1.5">
					Item Markup %
				</label>
				<Input id="equipment-item-markup" type="number" bind:value={model.markupPercent} min={100} max={10000} class="w-24" />
				<p class="text-xs text-text-tertiary mt-1">
					Replacement cost for limited items. 200% means each PEC of decay costs 2 PEC to replace.
				</p>
			</div>
		{/if}

		<!-- Actions -->
		<div class="flex items-center justify-end gap-2 pt-2">
			<Button type="button" variant="ghost" onclick={() => (model.showAddModal = false)}>Cancel</Button>
			<Button type="button" disabled={saveDisabled} onclick={() => model.saveEquipment()}>
				{model.saving ? 'Saving…' : model.editingEquipmentId ? 'Save Changes' : 'Save'}
			</Button>
		</div>
	</div>
</Modal>
