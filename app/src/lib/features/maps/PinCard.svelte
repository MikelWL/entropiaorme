<script lang="ts">
	/**
	 * The pin detail card: one popover surface serving marker hover AND
	 * keyboard focus, carrying the detail plus the pin's edit/delete
	 * actions. Positioned beside its marker and flipped
	 * to stay inside the view.
	 */
	import Button from '$lib/components/Button.svelte';
	import type { MapPin } from '$lib/api';
	import { formatGamePoint } from './coords';
	import { pinGlyph } from './pinIcons';

	let {
		pin,
		x,
		y,
		viewW,
		viewH,
		technicalName,
		onpointerenter,
		onpointerleave,
		onedit,
		ondelete,
	}: {
		pin: MapPin;
		x: number;
		y: number;
		viewW: number;
		viewH: number;
		technicalName: string | null;
		onpointerenter: () => void;
		onpointerleave: () => void;
		onedit: () => void;
		ondelete: () => void;
	} = $props();

	const CARD_W = 232;
	const CARD_MARGIN = 8;

	// Flip horizontally/vertically near the view edges.
	const left = $derived(
		Math.max(CARD_MARGIN, Math.min(x + 14, viewW - CARD_W - CARD_MARGIN)),
	);
	const openUp = $derived(y > viewH * 0.55);

	const createdLabel = $derived(
		new Date(pin.createdAt * 1000).toLocaleDateString(undefined, {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
		}),
	);
</script>

<div
	class="absolute z-10 w-58 rounded-lg border border-border bg-surface/95 p-3 text-sm shadow-lg backdrop-blur"
	style="left: {left}px; {openUp ? `bottom: ${viewH - y + 26}px;` : `top: ${y + 10}px;`} width: {CARD_W}px;"
	role="dialog"
	tabindex="-1"
	aria-label="Pin detail: {pin.name}"
	{onpointerenter}
	{onpointerleave}
	onfocusin={onpointerenter}
	onfocusout={onpointerleave}
	onpointerdown={(event) => event.stopPropagation()}
>
	<div class="flex items-start gap-2">
		<span class="text-lg leading-none" aria-hidden="true">{pinGlyph(pin.icon)}</span>
		<div class="min-w-0">
			<p class="font-medium text-text truncate">{pin.name}</p>
			<p class="text-xs text-text-secondary">{pin.kind}</p>
		</div>
	</div>

	<dl class="mt-2 space-y-1 text-xs text-text-secondary">
		<div class="flex justify-between gap-2">
			<dt>Position</dt>
			<dd class="tabular-nums text-text">{formatGamePoint({ lon: pin.lon, lat: pin.lat })}</dd>
		</div>
		{#if pin.radiusM != null}
			<div class="flex justify-between gap-2">
				<dt>Radius</dt>
				<dd class="tabular-nums text-text">{pin.radiusM} m</dd>
			</div>
		{/if}
		<div class="flex justify-between gap-2">
			<dt>Created</dt>
			<dd class="text-text">{createdLabel}</dd>
		</div>
	</dl>

	{#if pin.notes}
		<p class="mt-2 text-xs text-text-secondary whitespace-pre-wrap break-words">{pin.notes}</p>
	{/if}

	<p class="mt-3 text-xs text-text-secondary">
		{technicalName
			? 'Click the pin to copy its waypoint.'
			: 'This map cannot form an in-game waypoint.'}
	</p>

	<div class="mt-2 flex items-center gap-1.5">
		<Button size="sm" variant="ghost" onclick={onedit}>Edit</Button>
		<Button size="sm" variant="danger" onclick={ondelete}>Delete</Button>
	</div>
</div>
