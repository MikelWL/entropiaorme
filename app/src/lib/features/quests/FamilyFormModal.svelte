<script lang="ts">
	import { Button, Input, Modal, SegmentedControl, Select } from '$lib/components';
	import type { FamilyModel } from './familyModel.svelte';
	import { PLANETS, type CooldownUnit } from './questsModel.svelte';

	let { model }: { model: FamilyModel } = $props();
</script>

<Modal bind:open={model.showFamilyModal} title={model.editingFamily ? 'Edit Family' : 'New Family'} class="max-w-md">
	{#snippet children()}
		<form class="space-y-3" onsubmit={(e) => { e.preventDefault(); model.saveFamily(); }}>
			<div>
				<label class="block text-xs text-text-secondary mb-1" for="f-name">Name</label>
				<Input id="f-name" type="text" required bind:value={model.familyForm.name}
					placeholder="e.g., ARIS - Daily Hunting 1" />
				<p class="text-[11px] text-text-secondary/70 mt-1">
					Quests named "{model.familyForm.name.trim() || 'Family'}: Variant" attach automatically, including ones received in game later.
				</p>
			</div>
			<div>
				<label class="block text-xs text-text-secondary mb-1" for="f-planet">Planet</label>
				<Select id="f-planet" bind:value={model.familyForm.planet}>
					{#each PLANETS as planet}
						<option value={planet}>{planet}</option>
					{/each}
				</Select>
			</div>
			<div>
				<label class="block text-xs text-text-secondary mb-1" for="f-cd">Shared cooldown</label>
				<div class="flex items-stretch gap-2">
					<Input id="f-cd" type="number" step="1" min="0" bind:value={model.cooldownInput}
						class="flex-1 min-w-0"
						placeholder={model.cooldownUnit === 'hours' ? '20' : '1'} />
					<SegmentedControl
						size="md"
						options={[
							{ id: 'hours', label: 'Hours' },
							{ id: 'days', label: 'Days' }
						]}
						active={model.cooldownUnit}
						onchange={(id) => {
							if (id === 'hours' && model.cooldownUnit === 'days' && model.cooldownInput != null) {
								model.cooldownInput = model.cooldownInput * 24;
							} else if (id === 'days' && model.cooldownUnit === 'hours' && model.cooldownInput != null) {
								model.cooldownInput = Math.round((model.cooldownInput / 24) * 10) / 10;
							}
							model.cooldownUnit = id as CooldownUnit;
						}}
					/>
				</div>
				<p class="text-[11px] text-text-secondary/70 mt-1">
					One timer for the whole family: doing any variant gates every sibling. Leave empty to group without gating.
				</p>
			</div>
			<div>
				<div class="block text-xs text-text-secondary mb-1">Cooldown starts</div>
				<SegmentedControl
					size="md"
					options={[
						{ id: 'pickup', label: 'On pickup' },
						{ id: 'completion', label: 'On completion' }
					]}
					active={model.familyForm.cooldown_anchor}
					onchange={(id) => (model.familyForm.cooldown_anchor = id as 'pickup' | 'completion')}
				/>
				<p class="text-[11px] text-text-secondary/70 mt-1">
					{#if model.familyForm.cooldown_anchor === 'pickup'}
						Timer runs from collecting the mission at the giver; abandoning or completing it does not restart the wait. This matches the observed daily-slot behaviour.
					{:else}
						Timer runs from completing the variant.
					{/if}
				</p>
			</div>

			<div class="flex justify-end gap-2 pt-1">
				<Button type="button" variant="ghost" onclick={() => (model.showFamilyModal = false)}>{#snippet children()}Cancel{/snippet}</Button>
				<Button type="submit">{#snippet children()}{model.editingFamily ? 'Save' : 'Create'}{/snippet}</Button>
			</div>
		</form>
	{/snippet}
</Modal>
