<script lang="ts">
	import { untrack } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import Select from '$lib/components/Select.svelte';
	import { PIN_ICONS } from './pinIcons';
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
		if (open) buttons = untrack(() => config.buttons.map((button) => ({ ...button })));
	});

	function addButton() {
		if (buttons.length >= MAX_CARTOGRAPHY_BUTTONS) return;
		buttons = [
			...buttons,
			{
				id: globalThis.crypto?.randomUUID?.() ?? `button-${Date.now()}`,
				name: 'Pin',
				icon: 'pin',
				kind: 'marker',
				radiusM: null,
			},
		];
	}

	async function save() {
		saving = true;
		try {
			await setCartographyOverlayConfig({ planet: config.planet, buttons });
			open = false;
		} finally {
			saving = false;
		}
	}
</script>

<Modal bind:open title="Configure pin overlay" class="max-w-2xl! overflow-hidden">
	<div class="flex max-h-[calc(100vh-12rem)] min-h-0 flex-col">
		<div class="min-h-0 space-y-3 overflow-y-auto pr-2">
			{#each buttons as button, index (button.id)}
				<div class="rounded-md border border-border bg-surface/40 p-3">
					<div class="mb-2 flex items-center justify-between gap-3">
						<span class="text-xs font-medium text-text-secondary">Button {index + 1}</span>
						<Button
							size="sm"
							variant="ghost"
							disabled={buttons.length === 1}
							onclick={() => (buttons = buttons.filter((_, itemIndex) => itemIndex !== index))}
						>
							Remove
						</Button>
					</div>

					<div class="grid grid-cols-2 gap-3">
						<label class="min-w-0 space-y-1">
							<span class="text-xs text-text-secondary">Name</span>
							<Input bind:value={button.name} maxlength={40} />
						</label>
						<label class="min-w-0 space-y-1">
							<span class="text-xs text-text-secondary">Category</span>
							<Input bind:value={button.kind} maxlength={32} />
						</label>
						<label class="min-w-0 space-y-1">
							<span class="text-xs text-text-secondary">Icon</span>
							<Select bind:value={button.icon}>
								{#each PIN_ICONS as icon (icon.id)}
									<option value={icon.id}>{icon.glyph} {icon.label}</option>
								{/each}
							</Select>
						</label>
						<label class="min-w-0 space-y-1">
							<span class="text-xs text-text-secondary">Marks</span>
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
