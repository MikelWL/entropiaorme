<script lang="ts">
	import { Button, Input, Modal, SegmentedControl, Select } from '$lib/components';
	import { PLANETS, type CooldownUnit, type QuestsModel } from './questsModel.svelte';

	let { model }: { model: QuestsModel } = $props();
</script>

<Modal bind:open={model.showQuestModal} title={model.editingQuest ? 'Edit Quest' : 'New Quest'} class="max-w-lg">
	{#snippet children()}
		<form class="space-y-3" onsubmit={(e) => { e.preventDefault(); model.saveQuest(); }}>
			<div class="grid grid-cols-2 gap-3">
				<div class="col-span-2">
					<label class="block text-xs text-text-secondary mb-1" for="q-name">Name</label>
					<Input id="q-name" type="text" required bind:value={model.questForm.name}
						placeholder="e.g., Atlas Haven Imperium Ranger Hunt!" />
				</div>
				<div>
					<label class="block text-xs text-text-secondary mb-1" for="q-planet">Planet</label>
					<Select id="q-planet" bind:value={model.questForm.planet}>
						{#each PLANETS as planet}
							<option value={planet}>{planet}</option>
						{/each}
					</Select>
				</div>
				<div>
					<label class="block text-xs text-text-secondary mb-1" for="q-cat">Category</label>
					<Input id="q-cat" type="text" bind:value={model.questForm.category}
						placeholder="e.g., A.R.C. Faction" />
				</div>
				<div>
					<label class="block text-xs text-text-secondary mb-1" for="q-reward">Reward ({model.questForm.reward_is_skill ? 'PES' : 'PED'})</label>
					<Input id="q-reward" type="number" step="0.01" min="0" bind:value={model.questForm.reward_ped} />
				</div>
				<div>
					<div class="block text-xs text-text-secondary mb-1" aria-hidden="true">&nbsp;</div>
					<label class="flex items-center gap-1.5 h-[38px] text-xs text-text-secondary cursor-pointer">
						<input type="checkbox" bind:checked={model.questForm.reward_is_skill} class="accent-accent" />
						Reward is PES (skills)
					</label>
				</div>
				{#if !model.questForm.reward_is_skill}
					<div>
						<label class="block text-xs text-text-secondary mb-1" for="q-rmarkup">Expected Reward Markup %</label>
						<Input
							id="q-rmarkup"
							type="number"
							step="0.1"
							min="0"
							bind:value={model.questForm.expected_reward_markup_percent}
							disabled={model.rewardMarkupInputDisabled()}
							placeholder="e.g. 130"
						/>
					</div>
				{/if}
				<div class="col-span-2">
					<label class="block text-xs text-text-secondary mb-1" for="q-rdesc">Reward Note</label>
					<Input id="q-rdesc" type="text" bind:value={model.questForm.reward_description}
						placeholder="e.g., 3x A.R.C. Faction Badge" />
				</div>
				<div>
					<label class="block text-xs text-text-secondary mb-1" for="q-cd">Cooldown</label>
					<div class="flex items-stretch gap-2">
						<Input id="q-cd" type="number" step="1" min="0" bind:value={model.cooldownInput}
							class="flex-1 min-w-0"
							placeholder={model.cooldownUnit === 'hours' ? '21' : '7'} />
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
				</div>
				<div>
					<label class="block text-xs text-text-secondary mb-1" for="q-wp">Waypoint</label>
					<Input id="q-wp" type="text" bind:value={model.questForm.waypoint}
						class="font-mono"
						placeholder="/wp [Planet, Lon, Lat, Alt]" />
				</div>
			</div>

			<!-- Signal loot: set makes this a signal-completed quest (an
				 instance boss with no mission-log presence): focusing it in
				 the overlay starts a run, and this item's arrival in loot
				 completes it. Exclusive with a fixed reward, because the
				 boss's drop IS the reward and tracking already counts it. -->
			<div>
				<label class="block text-xs text-text-secondary mb-1" for="q-signal">Signal Loot (auto-complete)</label>
				<Input id="q-signal" type="text" bind:value={model.questForm.signal_loot_item}
					disabled={(model.questForm.reward_ped ?? 0) > 0}
					placeholder="e.g., Hyperion Daily Voucher" />
				<p class="text-[11px] text-text-secondary/70 mt-1">
					{#if (model.questForm.reward_ped ?? 0) > 0}
						Unavailable with a fixed reward: a signal quest's reward is its loot, which tracking already counts.
					{:else if model.questForm.signal_loot_item.trim()}
						Completes when this item drops outside a mission completion; focusing it in the overlay starts a run.
					{:else}
						Optional. For repeatable runs with no mission log entry (instance bosses): name the loot item whose drop marks completion.
					{/if}
				</p>
			</div>

			<!-- Target Mobs -->
			<div>
				<div class="block text-xs text-text-secondary mb-1">Target Mobs</div>
				{#if model.questForm.mobs.length > 0}
					<div class="flex flex-wrap gap-1 mb-1.5">
						{#each model.questForm.mobs as mob}
							<span class="text-xs px-2 py-0.5 rounded-full bg-accent/10 text-accent border border-accent/20 flex items-center gap-1">
								{mob}
								<button type="button" class="hover:text-text cursor-pointer" aria-label="Remove {mob}" onclick={() => model.removeMob(mob)}>&times;</button>
							</span>
						{/each}
					</div>
				{/if}
				<div class="flex gap-2">
					<Input type="text" bind:value={model.mobInput} placeholder="Type mob name, press Enter"
						class="flex-1"
						onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); model.addMob(); } }} />
					<Button type="button" size="sm" variant="secondary" onclick={() => model.addMob()}>{#snippet children()}Add{/snippet}</Button>
				</div>
			</div>


			<div class="flex justify-end gap-2 pt-1">
				<Button type="button" variant="ghost" onclick={() => (model.showQuestModal = false)}>{#snippet children()}Cancel{/snippet}</Button>
				<Button type="submit">{#snippet children()}{model.editingQuest ? 'Save' : 'Create'}{/snippet}</Button>
			</div>
		</form>
	{/snippet}
</Modal>
