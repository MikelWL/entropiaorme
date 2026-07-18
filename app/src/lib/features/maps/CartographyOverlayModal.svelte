<script lang="ts">
	import { untrack } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import Select from '$lib/components/Select.svelte';
	import EmojiPicker from './EmojiPicker.svelte';
	import { pinGlyph } from './pinIcons';
	import {
		MAX_CARTOGRAPHY_BUTTONS,
		setCartographyOverlayConfig,
		type CartographyButton,
		type CartographyOverlayConfig,
	} from './cartographyOverlay.svelte';

	let { open = $bindable(false), config }: { open?: boolean; config: CartographyOverlayConfig } =
		$props();
	let buttons = $state<CartographyButton[]>([]);
	let saving = $state(false);

	$effect(() => {
		if (open) {
			buttons = untrack(() => config.buttons.map((button) => ({ ...button })));
		}
	});

	function addButton() {
		if (buttons.length >= MAX_CARTOGRAPHY_BUTTONS) return;
		buttons = [
			...buttons,
			{
				id: globalThis.crypto?.randomUUID?.() ?? `button-${Date.now()}`,
				name: 'Pin',
				icon: '📍',
				kind: 'marker',
				radiusM: null,
			},
		];
	}

	function moveButton(index: number, offset: -1 | 1) {
		const target = index + offset;
		if (target < 0 || target >= buttons.length) return;
		const reordered = [...buttons];
		[reordered[index], reordered[target]] = [reordered[target], reordered[index]];
		buttons = reordered;
	}

	async function save() {
		saving = true;
		try {
			await setCartographyOverlayConfig({
				planet: config.planet,
				mapViewId: config.mapViewId,
				buttons: buttons.map((button) => ({
					...button,
					icon: pinGlyph(button.icon),
					kind: 'marker',
				})),
			});
			open = false;
		} finally {
			saving = false;
		}
	}
</script>

<Modal bind:open title="Configure pin overlay" class="max-w-xl! overflow-hidden">
	<div class="flex max-h-[calc(100vh-12rem)] min-h-0 flex-col">
		<div class="min-h-0 space-y-3 overflow-y-auto pr-2">
			{#each buttons as button, index (button.id)}
				<div class="rounded-md border border-border bg-surface/40 p-3">
					<div class="mb-2 flex items-center justify-between gap-3">
						<span class="text-xs font-medium text-text-secondary">Button {index + 1}</span>
						<div class="flex items-center gap-1">
							<Button size="sm" variant="ghost" disabled={index === 0} aria-label="Move {button.name} up" onclick={() => moveButton(index, -1)}>↑</Button>
							<Button size="sm" variant="ghost" disabled={index === buttons.length - 1} aria-label="Move {button.name} down" onclick={() => moveButton(index, 1)}>↓</Button>
							<Button
								size="sm"
								variant="ghost"
								disabled={buttons.length === 1}
								aria-label="Remove {button.name}"
								onclick={() => (buttons = buttons.filter((_, itemIndex) => itemIndex !== index))}
							>
								Remove
							</Button>
						</div>
					</div>

					<div class="grid grid-cols-1 gap-3 sm:grid-cols-[minmax(0,1fr)_2.25rem_minmax(0,1fr)]">
						<label class="min-w-0 space-y-1">
							<span class="block text-xs text-text-secondary">Name</span>
							<Input bind:value={button.name} maxlength={40} />
						</label>
						<EmojiPicker
							value={button.icon}
							label="Choose emoji for {button.name}"
							onselect={(emoji) => (button.icon = emoji)}
						/>
						<label class="min-w-0 space-y-1">
							<span class="block text-xs text-text-secondary">Area</span>
							<Select
								value={button.radiusM == null ? '' : String(button.radiusM)}
								onchange={(event) => {
									const value = (event.currentTarget as HTMLSelectElement).value;
									button.radiusM = value ? Number(value) : null;
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
				</div>
			{/each}
		</div>

		<div class="mt-3 flex shrink-0 items-center justify-between gap-3 border-t border-border/50 pt-3">
			<Button size="sm" variant="secondary" disabled={buttons.length >= MAX_CARTOGRAPHY_BUTTONS} onclick={addButton}>
				Add button
			</Button>
			<div class="flex gap-2">
				<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
				<Button loading={saving} onclick={save}>Save</Button>
			</div>
		</div>
	</div>
</Modal>
