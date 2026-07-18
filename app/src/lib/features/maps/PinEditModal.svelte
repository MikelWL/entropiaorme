<script lang="ts">
	/**
	 * Create/edit form for a pin, over the shared Modal primitive. The
	 * coordinates are shown but not editable here: a pin's position comes
	 * from the map click (or, later, the coordinate scan); moving a pin
	 * is a new drop.
	 */
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import Select from '$lib/components/Select.svelte';
	import type { MapPin } from '$lib/api';
	import { formatGamePoint, type GamePoint } from './coords';
	import { PIN_ICONS, pinKind } from './pinIcons';

	export interface PinFormValues {
		name: string;
		icon: string;
		kind: string;
		radiusM: number | null;
		notes: string;
	}

	let {
		open = $bindable(false),
		point,
		editing = null,
		onsubmit,
	}: {
		open?: boolean;
		/** The drop position (create mode) or the pin's position (edit). */
		point: GamePoint;
		/** The pin being edited, or null when creating. */
		editing?: MapPin | null;
		/** Resolves true when the pin persisted; false keeps the modal
		 * open so a failed save cannot discard the entered form. */
		onsubmit: (values: PinFormValues) => Promise<boolean>;
	} = $props();

	const RADIUS_PRESETS = [
		{ value: '', label: 'Exact spot' },
		{ value: '10', label: '10 m area' },
		{ value: '50', label: '50 m area' },
		{ value: '100', label: '100 m area' },
	];

	let name = $state('');
	let icon = $state('pin');
	let radius = $state('');
	let notes = $state('');

	// Re-seed the form whenever the modal opens for a target.
	$effect(() => {
		if (!open) return;
		name = editing?.name ?? '';
		icon = editing?.icon ?? 'pin';
		radius = editing?.radiusM == null ? '' : String(editing.radiusM);
		notes = editing?.notes ?? '';
	});

	const valid = $derived(name.trim().length > 0);
	let saving = $state(false);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!valid || saving) return;
		saving = true;
		try {
			const persisted = await onsubmit({
				name: name.trim(),
				icon,
				kind: pinKind(icon),
				radiusM: radius === '' ? null : Number(radius),
				notes: notes.trim(),
			});
			if (persisted) {
				open = false;
			}
		} finally {
			saving = false;
		}
	}
</script>

<Modal bind:open title={editing ? 'Edit pin' : 'Drop a pin'}>
	<form class="space-y-4" onsubmit={submit}>
		<p class="text-xs text-text-secondary tabular-nums">
			Position: {formatGamePoint(point)}
		</p>

		<label class="block space-y-1">
			<span class="text-xs text-text-secondary">Name</span>
			<Input bind:value={name} placeholder="e.g. Ore claim north ridge" required />
		</label>

		<div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
			<label class="block space-y-1">
				<span class="text-xs text-text-secondary">Marker</span>
				<Select bind:value={icon}>
					{#each PIN_ICONS as def (def.id)}
						<option value={def.id}>{def.glyph} {def.label}</option>
					{/each}
				</Select>
			</label>
			<label class="block space-y-1">
				<span class="text-xs text-text-secondary">Area</span>
				<Select bind:value={radius}>
					{#each RADIUS_PRESETS as preset (preset.value)}
						<option value={preset.value}>{preset.label}</option>
					{/each}
				</Select>
			</label>
		</div>

		<label class="block space-y-1">
			<span class="text-xs text-text-secondary">Notes</span>
			<textarea
				bind:value={notes}
				rows="3"
				class="w-full rounded-md border border-border bg-surface/70 px-3 py-2 text-sm text-text placeholder:text-text-tertiary focus:outline-none focus:border-accent/60"
				placeholder="Optional notes"
			></textarea>
		</label>

		<div class="flex justify-end gap-2">
			<Button type="button" variant="ghost" onclick={() => (open = false)}>Cancel</Button>
			<Button type="submit" disabled={!valid} loading={saving}>
				{editing ? 'Save' : 'Drop pin'}
			</Button>
		</div>
	</form>
</Modal>
