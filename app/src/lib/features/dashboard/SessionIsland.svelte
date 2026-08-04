<script lang="ts">
	import { flip } from 'svelte/animate';
	import { quintOut } from 'svelte/easing';
	import { startTracking, toggleOverlay } from '$lib/api';
	import type { TrackingSnapshot } from '$lib/api';
	import { Button, ErrorNotice } from '$lib/components';
	import DefinitionPicker from '$lib/features/sessions/DefinitionPicker.svelte';
	import type { DefinitionsModel } from '$lib/features/sessions/definitionsModel.svelte';
	import { shouldSettleInstantly } from '$lib/motion/testMotion';
	import { useVisiblePoll } from '$lib/realtime/useVisiblePoll';
	import { hydrate } from '$lib/stores/trackingStore.svelte';
	import { getStatDef } from '$lib/statsRegistry';
	import { describeError } from '$lib/view/errorState';
	import type { StatsGridModel } from './statsGridModel.svelte';

	let {
		status,
		statsGrid,
		definitions
	}: {
		status: TrackingSnapshot | null;
		statsGrid: StatsGridModel;
		/** The sessions model; the route hosts it (and the authoring
		 * environment) so the surface can replace the whole dashboard. */
		definitions: DefinitionsModel;
	} = $props();

	let elapsedSeconds = $state(0);

	function openAuthoring(definitionId: string | null) {
		const editing =
			definitionId === null
				? null
				: definitions.definitions.find((definition) => definition.id === definitionId);
		if (editing) definitions.openEdit(editing);
		else definitions.openCreate();
	}

	const isActive = $derived(status?.status === 'active');
	const selectedDefinitionId = $derived(status?.sessionDefinitionId ?? null);

	let starting = $state(false);
	let startError = $state<string | null>(null);

	async function handleStart() {
		starting = true;
		startError = null;
		try {
			await startTracking();
			await hydrate();
		} catch (e) {
			startError = describeError(e, 'Failed to start the session');
		} finally {
			starting = false;
		}
	}

	// Elapsed timer when tracking is active
	$effect(() => {
		if (status?.status === 'active' && status.started_at) {
			const startMs = new Date(status.started_at).getTime();
			elapsedSeconds = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
			return useVisiblePoll(() => {
				elapsedSeconds = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
			}, { intervalMs: 1000, immediate: false });
		} else {
			elapsedSeconds = 0;
		}
	});

	function formatElapsed(seconds: number): string {
		const h = Math.floor(seconds / 3600);
		const m = Math.floor((seconds % 3600) / 60);
		return h > 0 ? `${h}h ${m}m` : `${m}m`;
	}
</script>

<!-- Island: Session -->
<section class="panel p-4 flex flex-col gap-3 flex-shrink-0">
	<!-- Session strip -->
	<div class="relative flex items-center justify-between">
		{#if status?.status === 'active'}
			<div class="flex items-center gap-3 min-w-0">
				<span class="signal-dot positive animate-pulse"></span>
				<span class="text-sm font-medium text-text tracking-tight">Tracking active</span>
				{#if status.sessionName}
					<span
						class="text-xs text-text-secondary truncate"
						title="{status.sessionName} (fixed for this session)"
					>
						{status.sessionName}
					</span>
				{/if}
				<span
					class="text-xs text-text-tertiary tabular-nums tracking-wider"
					data-testid="session-elapsed"
				>
					{formatElapsed(elapsedSeconds)}
				</span>
			</div>
		{:else}
			<!-- At rest the island is titled by the session it will run as;
				 the stats below already read @Rest, so nothing states it twice. -->
			<DefinitionPicker
				model={definitions}
				selectedId={selectedDefinitionId}
				onOpenAuthoring={openAuthoring}
			/>
		{/if}

		<div class="flex items-center gap-2">
			{#if !isActive}
				<Button size="sm" disabled={starting} onclick={handleStart}>
					{#snippet children()}{starting ? 'Starting...' : 'Start'}{/snippet}
				</Button>
			{/if}
			<span class="inline-flex" data-guide-anchor="dashboard-overlay-btn">
				<Button size="sm" variant={isActive ? 'primary' : 'secondary'} onclick={() => toggleOverlay().catch(() => {})}>
					{#snippet children()}Overlay{/snippet}
				</Button>
			</span>
		</div>
	</div>

	<ErrorNotice
		message={startError ?? definitions.error}
		onDismiss={() => {
			startError = null;
			definitions.error = null;
		}}
	/>

	<!-- Session stats -->
	<div
		class="dashboard-stat-grid relative grid gap-2"
		data-guide-anchor="dashboard-stats-grid"
	>
		{#each statsGrid.enabledStats as pref, i (pref.id)}
			{@const def = getStatDef(pref.id)}
			{@const r = def ? def.render(status) : { value: '\u2014', color: 'text-text-tertiary' }}
			{@const isDragged = statsGrid.dragFilteredIndex === i}
			<div
				animate:flip={{ duration: shouldSettleInstantly() ? 0 : 240, easing: quintOut }}
				data-stat-cell={i}
				role="group"
				aria-label={def?.label ?? pref.id}
				class="relative rounded-md border border-border/60 bg-base/40 px-3 py-2.5 flex flex-col gap-1
					min-w-0 cursor-grab select-none touch-none
					transition-[opacity,box-shadow,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
					before:pointer-events-none before:absolute before:inset-0 before:rounded-[inherit]
					before:[box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.03)]
					{isDragged ? 'opacity-40 shadow-lg ring-1 ring-accent/60 z-10' : ''}"
				onpointerdown={(e) => statsGrid.handlePointerDown(e, i)}
				onpointermove={statsGrid.handlePointerMove}
				onpointerup={statsGrid.handlePointerUp}
				onpointercancel={statsGrid.handlePointerCancel}
			>
				<span class="eyebrow truncate">{def?.shortLabel ?? def?.label ?? pref.id}</span>
				<span class="truncate text-[17px] font-semibold tabular-nums leading-none tracking-tight
					{r.value === '\u2014' ? 'text-text-tertiary' : r.color}">
					{r.value}
				</span>
			</div>
		{/each}
	</div>

	{#if status?.status === 'active'}
		{#if status.weaponAttribution === 'hotbar' && status.hotbarListenerActive === false}
			<div class="relative flex items-start gap-3 px-3.5 py-3 rounded-md border border-warning/30 bg-warning/[0.06]">
				<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-4.5 w-4.5 mt-0.5 text-warning shrink-0">
					<path fill-rule="evenodd" d="M8.485 2.495c.673-1.167 2.357-1.167 3.03 0l6.28 10.875c.673 1.167-.17 2.625-1.516 2.625H3.72c-1.347 0-2.189-1.458-1.515-2.625L8.485 2.495zM10 5a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 0110 5zm0 9a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
				</svg>
				<div class="flex flex-col gap-0.5">
					<p class="text-sm font-medium text-warning tracking-tight">Hotbar key listener not active</p>
					<p class="text-xs text-text-secondary leading-relaxed">
						Cost attribution is using the hotbar but the listener isn't running. Check that the hotbar key listener is enabled in Settings.
					</p>
				</div>
			</div>
		{/if}
	{/if}
</section>

<style>
	.dashboard-stat-grid {
		grid-template-columns: repeat(
			auto-fill,
			minmax(clamp(112px, calc((100% - 2rem) / 5), 140px), 1fr)
		);
	}

	:global(body.stat-drag-active),
	:global(body.stat-drag-active *) {
		cursor: grabbing !important;
		user-select: none;
	}
</style>
