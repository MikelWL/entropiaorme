<script lang="ts">
	import { untrack } from 'svelte';
	import {
		createPinConfig,
		deletePinConfig,
		getPinConfigs,
		reorderPinConfigs,
		updatePinConfig,
		type PinConfig,
	} from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import Select from '$lib/components/Select.svelte';
	import EmojiPicker from './EmojiPicker.svelte';
	import { pinGlyph } from './pinIcons';
	import {
		DEFAULT_GENERIC_COLOUR,
		DEFAULT_TREE_COLOUR,
		DEFAULT_TREE_COOLDOWN_COLOUR,
		MAX_PIN_CONFIGS,
	} from './cartographyOverlay.svelte';

	let {
		open = $bindable(false),
		planet,
		mapViewId,
		mapName,
		onchanged,
	}: {
		open?: boolean;
		planet: string | null;
		mapViewId: number | null;
		mapName: string;
		onchanged: () => void;
	} = $props();

	type Draft = {
		key: string;
		id: number | null;
		label: string;
		category: 'generic' | 'special';
		specialKind: string | null;
		icon: string;
		radiusM: number | null;
		colour: string;
		cooldownColour: string | null;
		placedCount: number;
	};

	let drafts = $state<Draft[]>([]);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let pendingDelete = $state<string | null>(null);

	function toDraft(config: PinConfig): Draft {
		return {
			key: `config-${config.id}`,
			id: config.id,
			label: config.label,
			category: config.category === 'special' ? 'special' : 'generic',
			specialKind: config.specialKind ?? null,
			icon: config.icon,
			radiusM: config.radiusM,
			colour: config.colour,
			cooldownColour: config.cooldownColour,
			placedCount: config.placedCount,
		};
	}

	$effect(() => {
		if (!open) return;
		const currentPlanet = planet;
		const currentView = mapViewId;
		untrack(() => {
			error = null;
			pendingDelete = null;
			if (!currentPlanet) {
				drafts = [];
				return;
			}
			void getPinConfigs(currentPlanet, currentView)
				.then((configs) => (drafts = configs.map(toDraft)))
				.catch(() => (error = 'The pin configurations could not be loaded.'));
		});
	});

	function addDraft(category: 'generic' | 'special') {
		if (drafts.length >= MAX_PIN_CONFIGS) return;
		const special = category === 'special';
		drafts = [
			...drafts,
			{
				key: `new-${globalThis.crypto?.randomUUID?.() ?? Date.now()}-${drafts.length}`,
				id: null,
				label: special ? 'Tree' : 'Pin',
				category,
				specialKind: special ? 'tree' : null,
				icon: special ? '🌳' : '📍',
				radiusM: null,
				colour: special ? DEFAULT_TREE_COLOUR : DEFAULT_GENERIC_COLOUR,
				cooldownColour: special ? DEFAULT_TREE_COOLDOWN_COLOUR : null,
				placedCount: 0,
			},
		];
	}

	function setCategory(draft: Draft, category: 'generic' | 'special') {
		draft.category = category;
		if (category === 'special') {
			draft.specialKind = 'tree';
			draft.cooldownColour ??= DEFAULT_TREE_COOLDOWN_COLOUR;
		} else {
			draft.specialKind = null;
			draft.cooldownColour = null;
		}
	}

	function move(index: number, offset: -1 | 1) {
		const target = index + offset;
		if (target < 0 || target >= drafts.length) return;
		const reordered = [...drafts];
		[reordered[index], reordered[target]] = [reordered[target], reordered[index]];
		drafts = reordered;
	}

	async function removeDraft(draft: Draft) {
		if (draft.id == null) {
			drafts = drafts.filter((item) => item.key !== draft.key);
			pendingDelete = null;
			return;
		}
		saving = true;
		error = null;
		try {
			await deletePinConfig(draft.id);
			drafts = drafts.filter((item) => item.key !== draft.key);
			pendingDelete = null;
			onchanged();
		} catch {
			error = 'The pin option could not be removed.';
		} finally {
			saving = false;
		}
	}

	async function save() {
		if (!planet) return;
		saving = true;
		error = null;
		try {
			for (const draft of drafts) {
				const label = draft.label.trim() || (draft.category === 'special' ? 'Tree' : 'Pin');
				const icon = pinGlyph(draft.icon);
				const special = draft.category === 'special';
				const payload = {
					label,
					category: draft.category,
					specialKind: special ? 'tree' : null,
					icon,
					radiusM: draft.radiusM,
					colour: draft.colour,
					cooldownColour: special ? (draft.cooldownColour ?? DEFAULT_TREE_COOLDOWN_COLOUR) : null,
				};
				if (draft.id == null) {
					const created = await createPinConfig({ planet, mapViewId, ...payload });
					draft.id = created.id;
				} else {
					await updatePinConfig(draft.id, payload);
				}
			}
			await reorderPinConfigs(drafts.map((draft) => draft.id!).filter((id) => id != null));
			onchanged();
			open = false;
		} catch {
			error = 'The pin configurations could not be saved.';
		} finally {
			saving = false;
		}
	}
</script>

<Modal bind:open title="Configure pin overlay" class="max-w-2xl overflow-hidden">
	<p class="mb-2 text-xs text-text-secondary">Pins for {planet ?? 'no planet'} · {mapName}</p>
	<div class="flex max-h-[calc(100vh-13rem)] min-h-0 flex-col">
		<div class="min-h-0 space-y-3 overflow-y-auto pr-2">
			{#if drafts.length === 0}
				<p class="rounded-md border border-border bg-surface/40 p-4 text-center text-sm text-text-secondary">
					No pin options yet. Add a generic marker or a special tree to start.
				</p>
			{/if}
			{#each drafts as draft, index (draft.key)}
				<div class="rounded-md border border-border bg-surface/40 p-3">
					<div class="mb-2 flex items-center justify-between gap-3">
						<span class="text-lg" aria-hidden="true">{pinGlyph(draft.icon)}</span>
						<div class="flex items-center gap-1">
							<Button size="sm" variant="ghost" disabled={index === 0} aria-label="Move {draft.label} up" onclick={() => move(index, -1)}>↑</Button>
							<Button size="sm" variant="ghost" disabled={index === drafts.length - 1} aria-label="Move {draft.label} down" onclick={() => move(index, 1)}>↓</Button>
							<Button size="sm" variant="ghost" aria-label="Remove {draft.label}" onclick={() => (pendingDelete = draft.key)}>Remove</Button>
						</div>
					</div>

					{#if pendingDelete === draft.key}
						<div class="mb-2 rounded-md border border-danger/40 bg-danger/10 p-2">
							<p class="text-xs text-text">
								Remove the "{draft.label}" pin option?
								{#if draft.placedCount > 0}
									This also deletes the {draft.placedCount}
									{draft.placedCount === 1 ? 'pin' : 'pins'} already placed with it on this map. This can't be undone.
								{/if}
							</p>
							<div class="mt-2 flex justify-end gap-2">
								<Button size="sm" variant="ghost" onclick={() => (pendingDelete = null)}>Keep</Button>
								<Button size="sm" variant="danger" loading={saving} onclick={() => removeDraft(draft)}>Remove</Button>
							</div>
						</div>
					{/if}

					<div class="grid grid-cols-1 gap-3 sm:grid-cols-[7rem_minmax(0,1fr)_2.25rem_minmax(0,1fr)]">
						<label class="min-w-0 space-y-1">
							<span class="block text-xs text-text-secondary">Type</span>
							<Select
								value={draft.category}
								onchange={(event) => setCategory(draft, (event.currentTarget as HTMLSelectElement).value as 'generic' | 'special')}
							>
								<option value="generic">Generic</option>
								<option value="special">Tree</option>
							</Select>
						</label>
						<label class="min-w-0 space-y-1">
							<span class="block text-xs text-text-secondary">Name</span>
							<Input bind:value={draft.label} maxlength={40} />
						</label>
						<div class="space-y-1">
							<span class="block text-xs text-text-secondary">Icon</span>
							<EmojiPicker
								value={draft.icon}
								label="Choose emoji for {draft.label}"
								onselect={(emoji) => (draft.icon = emoji)}
							/>
						</div>
						<label class="min-w-0 space-y-1">
							<span class="block text-xs text-text-secondary">Area</span>
							<Select
								value={draft.radiusM == null ? '' : String(draft.radiusM)}
								onchange={(event) => {
									const value = (event.currentTarget as HTMLSelectElement).value;
									draft.radiusM = value ? Number(value) : null;
								}}
							>
								<option value="">Exact spot</option>
								<option value="10">10 m area</option>
								<option value="50">50 m area</option>
								<option value="100">100 m area</option>
								<option value="250">250 m area</option>
								<option value="500">500 m area</option>
								<option value="1000">1 km area</option>
							</Select>
						</label>
					</div>

					<div class="mt-3 flex flex-wrap items-center gap-4">
						<label class="flex items-center gap-2 text-xs text-text-secondary">
							<span>{draft.category === 'special' ? 'Available colour' : 'Colour'}</span>
							<input type="color" class="colour-input" aria-label="{draft.label} colour" bind:value={draft.colour} />
						</label>
						{#if draft.category === 'special'}
							<label class="flex items-center gap-2 text-xs text-text-secondary">
								<span>On-cooldown colour</span>
								<input
									type="color"
									class="colour-input"
									aria-label="{draft.label} cooldown colour"
									value={draft.cooldownColour ?? DEFAULT_TREE_COOLDOWN_COLOUR}
									oninput={(event) => (draft.cooldownColour = event.currentTarget.value)}
								/>
							</label>
						{/if}
					</div>
				</div>
			{/each}
		</div>

		{#if error}
			<p class="mt-2 shrink-0 text-xs text-danger" role="alert">{error}</p>
		{/if}

		<div class="mt-3 flex shrink-0 items-center justify-between gap-3 border-t border-border/50 pt-3">
			<div class="flex gap-2">
				<Button size="sm" variant="secondary" disabled={!planet || drafts.length >= MAX_PIN_CONFIGS} onclick={() => addDraft('generic')}>
					Add generic
				</Button>
				<Button size="sm" variant="secondary" disabled={!planet || drafts.length >= MAX_PIN_CONFIGS} onclick={() => addDraft('special')}>
					Add tree
				</Button>
			</div>
			<div class="flex gap-2">
				<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
				<Button loading={saving} disabled={!planet} onclick={save}>Save</Button>
			</div>
		</div>
	</div>
</Modal>

<style>
	.colour-input {
		width: 2rem;
		height: 1.75rem;
		border-radius: 0.375rem;
		border: 1px solid var(--color-border);
		background: transparent;
		padding: 0.125rem;
		cursor: pointer;
	}
</style>
