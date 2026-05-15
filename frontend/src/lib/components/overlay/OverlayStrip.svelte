<script lang="ts">
	import type {
		TrackingLive,
		TrackingStatus,
		SessionQuestLinkSuggestion
	} from '$lib/api';
	import { overlayStats } from '$lib/statsCustomisation';
	import { getStatDef } from '$lib/statsRegistry';
	import Button from '$lib/components/Button.svelte';
	import type { MobTrackingMode } from '$lib/types/settings';
	import TrifectaSelector from './TrifectaSelector.svelte';
	import { ICON_EQUIPMENT, ICON_ARMOUR } from './icons';

	type LastSessionStats = { cost: number; returns: number; pes: number; net: number };

	const noop = () => {};

	let {
		data,
		status = null,
		toggling = false,
		releasing = false,
		selectingMob = false,
		trifectaSaving = false,
		trifectaError = null,
		armourCostOpen = false,
		armourCostError = null,
		armourSessionId = null,
		mobMenuOpen = false,
		trifectaMenuOpen = false,
		overlayMenuLaunchError = null,
		lastSessionId = null,
		lastSessionStats = null,
		questLinkSuggestion = null,
		questLinkMessage = null,
		questLinkSaving = false,
		mobQuery = $bindable(''),
		mobInput = $bindable(null),
		postSessionArmourButton = $bindable(null),
		awaitingArmourTrackDecision = false,
		attributionWarning = null,
		onStart = noop,
		onStop = noop,
		onArmourTrackDecision = noop,
		onDismissAttributionWarning = noop,
		onMobModeChange = noop,
		onReleaseMob = noop,
		onMobFocus = noop,
		onMobBlur = noop,
		onMobKeydown = noop,
		onTrifectaTrigger = noop,
		onArmourCostToggle = noop,
		onQuestLinkDecision = noop,
		onDismissQuestLinkMessage = noop
	}: {
		data: TrackingLive;
		status?: TrackingStatus | null;
		toggling?: boolean;
		releasing?: boolean;
		selectingMob?: boolean;
		trifectaSaving?: boolean;
		trifectaError?: string | null;
		armourCostOpen?: boolean;
		armourCostError?: string | null;
		armourSessionId?: string | null;
		mobMenuOpen?: boolean;
		trifectaMenuOpen?: boolean;
		overlayMenuLaunchError?: string | null;
		lastSessionId?: string | null;
		lastSessionStats?: LastSessionStats | null;
		questLinkSuggestion?: SessionQuestLinkSuggestion | null;
		questLinkMessage?: string | null;
		questLinkSaving?: boolean;
		mobQuery?: string;
		mobInput?: HTMLInputElement | null;
		postSessionArmourButton?: HTMLButtonElement | null;
		awaitingArmourTrackDecision?: boolean;
		attributionWarning?: string | null;
		onStart?: () => void | Promise<void>;
		onStop?: () => void | Promise<void>;
		onArmourTrackDecision?: (action: 'yes' | 'no') => void | Promise<void>;
		onDismissAttributionWarning?: () => void;
		onMobModeChange?: (mode: MobTrackingMode) => void | Promise<void>;
		onReleaseMob?: () => void | Promise<void>;
		onMobFocus?: () => void;
		onMobBlur?: () => void;
		onMobKeydown?: (event: KeyboardEvent) => void | Promise<void>;
		onTrifectaTrigger?: (anchor: HTMLButtonElement) => void | Promise<void>;
		onArmourCostToggle?: (event: MouseEvent) => void | Promise<void>;
		onQuestLinkDecision?: (action: 'accept' | 'decline') => void | Promise<void>;
		onDismissQuestLinkMessage?: () => void;
	} = $props();

	const isTagEntryMode = $derived(data.mobEntryMode === 'tag');
	const isTrifectaAttribution = $derived(data.weaponAttribution === 'trifecta');
	const showTagInput = $derived(
		(data.status === 'active' || data.status === 'idle')
			&& isTagEntryMode
			&& !data.currentMob
	);
	const showManualMobInput = $derived(
		(data.status === 'active' || data.status === 'idle')
			&& !isTagEntryMode
			&& !data.currentMob
	);
	const showManualInput = $derived(showTagInput || showManualMobInput);

	function formatElapsed(seconds: number): string {
		const h = Math.floor(seconds / 3600);
		const m = Math.floor((seconds % 3600) / 60);
		const s = seconds % 60;
		if (h > 0) return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
		return `${m}:${s.toString().padStart(2, '0')}`;
	}

	function formatPed(v: number): string {
		return v.toFixed(2);
	}
</script>

<!-- Glassmorphic container -->
<div class="overlay-strip glass-panel flex items-center gap-3 rounded-xl px-4 py-2 w-max">
	{#if data.status === 'active' || !lastSessionId}
		<!-- Track Button + Timer -->
		<div class="flex items-center gap-3 shrink-0 border-r border-white/10 pr-3">
			{#if awaitingArmourTrackDecision && data.status === 'active'}
				<div class="armour-prompt flex items-center gap-1.5 shrink-0">
					<span class="text-[10px] font-semibold text-amber-300 tracking-wide whitespace-nowrap">Track armour?</span>
					<button
						type="button"
						class="armour-prompt-btn armour-prompt-yes"
						disabled={toggling}
						onclick={() => onArmourTrackDecision('yes')}
					>Yes</button>
					<button
						type="button"
						class="armour-prompt-btn armour-prompt-no"
						disabled={toggling}
						onclick={() => onArmourTrackDecision('no')}
					>No</button>
				</div>
			{:else if attributionWarning && data.status !== 'active'}
				<div class="armour-prompt flex items-center gap-2 shrink-0 max-w-[420px]">
					<span class="text-[10px] font-medium text-amber-200 leading-snug">{attributionWarning}</span>
					<button
						type="button"
						class="armour-prompt-btn armour-prompt-close"
						aria-label="Dismiss warning"
						onclick={() => onDismissAttributionWarning()}
					>×</button>
				</div>
			{:else}
				<button
					class={data.status === 'active' ? 'stop-btn' : 'start-btn'}
					disabled={toggling}
					onclick={data.status === 'active' ? onStop : onStart}
					title={data.status === 'active' ? 'Stop tracking' : 'Start tracking'}
				>
					{#if toggling}
						<span class="text-[10px] px-1">...</span>
					{:else if data.status === 'active'}
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-2.5 h-2.5">
							<rect x="3" y="3" width="10" height="10" rx="1" />
						</svg>
					{:else}
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3">
							<path d="M4 3.5a.5.5 0 0 1 .757-.429l8 4.8a.5.5 0 0 1 0 .858l-8 4.8A.5.5 0 0 1 4 13V3.5z" />
						</svg>
						<span class="font-bold tracking-wide">TRACK</span>
					{/if}
				</button>
			{/if}
			{#if data.status === 'active'}
				<div class="flex items-center gap-1.5">
					<span class="relative flex h-2 w-2 shrink-0">
						<span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
						<span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-400"></span>
					</span>
					<span class="text-sm font-semibold text-emerald-400 tabular-nums tracking-wider w-12 text-center">
						{formatElapsed(data.elapsed ?? 0)}
					</span>
				</div>
			{/if}
		</div>

		<!-- Mob/Tag Section. The segmented control always renders; during an
			 active session the buttons disable (mode is locked once a session
			 starts) but the highlighted segment still communicates which mode
			 is in effect. This is clearer than the prior label-only treatment
			 mid-hunt, and gives the guide's demo overlay a visible toggle. -->
		<div
			class="flex items-center gap-2 shrink-0 border-r border-white/10 pr-3"
			data-guide-anchor="overlay-mob-section"
		>
			<div class="flex items-center gap-0.5 rounded bg-white/5 p-0.5 shrink-0">
				<button
					type="button"
					class="px-2 py-0.5 rounded-[2px] text-[10px] font-bold transition-colors cursor-pointer {data.mobEntryMode !== 'tag' ? 'bg-accent/20 text-accent' : 'text-white/50 hover:text-white'} disabled:cursor-default"
					disabled={toggling || data.status === 'active'}
					onclick={() => onMobModeChange('mob')}
				>MOB</button>
				<button
					type="button"
					class="px-2 py-0.5 rounded-[2px] text-[10px] font-bold transition-colors cursor-pointer {data.mobEntryMode === 'tag' ? 'bg-accent/20 text-accent' : 'text-white/50 hover:text-white'} disabled:cursor-default"
					disabled={toggling || data.status === 'active'}
					onclick={() => onMobModeChange('tag')}
				>TAG</button>
			</div>

			<div class="w-32 flex flex-col shrink-0">
				<div class="flex items-center">
					{#if showManualInput}
						<input
							bind:this={mobInput}
							class="w-full bg-transparent border-b border-white/10 focus:border-accent text-sm text-white/90 px-1 py-0.5 outline-none placeholder:text-white/20 transition-colors"
							bind:value={mobQuery}
							placeholder={showTagInput ? 'Tag...' : 'Mob...'}
							disabled={selectingMob}
							onfocus={onMobFocus}
							onblur={onMobBlur}
							onkeydown={onMobKeydown}
						/>
					{:else if data.currentMob}
						<div class="text-sm font-medium text-white/90 truncate px-1 w-full">{data.currentMob}</div>
					{:else}
						<div class="text-sm font-medium text-white/20 px-1">—</div>
					{/if}
				</div>
				{#if showManualInput && overlayMenuLaunchError && !mobMenuOpen}
					<div class="mt-1 px-1 text-[10px] leading-tight text-orange-300/90">
						{overlayMenuLaunchError}
					</div>
				{/if}
			</div>
			{#if data.currentMob}
				<button
					type="button"
					class="release-btn shrink-0"
					aria-label="Release mob"
					onclick={onReleaseMob}
					title={isTagEntryMode ? 'Clear tag' : 'Release mob'}
				>
					{releasing ? '...' : 'x'}
				</button>
			{/if}
		</div>

		<!-- Trifecta/Weapon Section. No own separator; the adjacent armour section
			 owns the boundary via its left border. -->
		<div
			class="flex items-center gap-2 shrink-0"
			data-guide-anchor="overlay-equipment-section"
		>
			<span class="text-white/40 shrink-0">{@html ICON_EQUIPMENT}</span>
			{#if isTrifectaAttribution}
				<TrifectaSelector
					trifecta={data.trifectaAttribution}
					tone={data.status === 'active' ? 'active' : 'idle'}
					menuOpen={trifectaMenuOpen}
					disabled={trifectaSaving}
					error={trifectaError}
					ontrigger={onTrifectaTrigger}
				/>
			{:else}
				<div class="text-xs {data.currentTool ? 'text-white/70' : 'text-white/20'} truncate max-w-[120px]">
					{data.currentTool || '—'}
				</div>
			{/if}
		</div>

		<!-- Armour cost (standalone, user-initiated) -->
		<div
			class="flex flex-col shrink-0 border-l border-white/10 pl-3"
			data-guide-anchor="overlay-armour-section"
		>
			<div class="flex items-center gap-2 shrink-0">
				<span class="text-white/40 shrink-0">{@html ICON_ARMOUR}</span>
				<button
					class="px-2 py-0.5 rounded-[4px] border text-[9px] font-medium transition-all cursor-pointer
						{armourSessionId
							? armourCostOpen
								? 'bg-accent/20 border-accent/40 text-accent'
								: 'bg-white/5 border-white/10 text-white/60 hover:bg-white/10 hover:text-white/90'
							: 'bg-white/5 border-white/10 text-white/20 cursor-not-allowed'}"
					disabled={!armourSessionId}
					aria-haspopup="dialog"
					aria-expanded={armourCostOpen}
					onclick={onArmourCostToggle}
					title={armourSessionId ? 'Record armour cost' : 'Start or stop a session to enable'}
					data-guide-anchor="overlay-armour-cost-btn"
				>
					Cost
				</button>
			</div>
			{#if armourCostError && !armourCostOpen}
				<div class="mt-1 px-1 text-[10px] leading-tight text-orange-300/90">
					{armourCostError}
				</div>
			{/if}
		</div>

		<!-- Customisable stat pills (driven by $overlayStats): treated as one
			 unit, so the section separator sits at the unit boundary, not between
			 individual pills. -->
		{@const enabledPills = $overlayStats.filter((p) => p.enabled)}
		{#if enabledPills.length > 0}
			<div class="flex items-center gap-4 shrink-0 border-l border-white/10 pl-3">
				{#each enabledPills as pref (pref.id)}
					{@const def = getStatDef(pref.id)}
					{#if def}
						{@const r = def.render(status)}
						{@const valueColor = r.value === '—'
							? 'text-white/25'
							: r.color === 'text-text'
								? 'text-white/85'
								: r.color}
						<div class="flex flex-col items-center justify-center gap-0.5 shrink-0">
							<span class="text-[10px] font-bold text-white/40 tracking-wider uppercase leading-none">{def.label}</span>
							<span class="text-sm font-semibold tabular-nums leading-none {valueColor}">{r.value}</span>
						</div>
					{/if}
				{/each}
			</div>
		{/if}
	{:else}
		<!-- Post-session quest-link bar. Armour cost is decoupled: use the
			 Armour badge in the active-UI strip to record it before closing. -->
		<div class="flex items-center gap-4 shrink-0">
			<span class="text-[10px] font-bold text-white/60 tracking-wider uppercase shrink-0">Session ended</span>

			{#if lastSessionStats}
				<div class="flex items-center gap-3 px-3 border-x border-white/10 shrink-0">
					<div class="flex items-center gap-1.5">
						<span class="text-[9px] text-white/40 uppercase tracking-widest">Cycled</span>
						<span class="text-xs font-semibold text-orange-400 tabular-nums">{formatPed(lastSessionStats.cost)}</span>
					</div>
					<div class="flex items-center gap-1.5">
						<span class="text-[9px] text-white/40 uppercase tracking-widest">Net</span>
						<span class="text-xs font-semibold tabular-nums {lastSessionStats.net >= 0 ? 'text-emerald-400' : 'text-orange-400'}">
							{lastSessionStats.net >= 0 ? '+' : ''}{formatPed(lastSessionStats.net)}
						</span>
					</div>
				</div>
			{/if}

			{#if questLinkSuggestion}
				<div class="flex items-center gap-2 shrink-0">
					<span class="text-xs text-white/50">Quest:</span>
					<span class="text-sm text-white/80 max-w-[150px] truncate">
						{questLinkSuggestion.suggestionType === 'quest' ? questLinkSuggestion.questName : questLinkSuggestion.playlistName}
					</span>
					<Button variant="primary" size="sm" onclick={() => onQuestLinkDecision('accept')} disabled={questLinkSaving}>Yes</Button>
					<Button variant="ghost" size="sm" onclick={() => onQuestLinkDecision('decline')} disabled={questLinkSaving}>No</Button>
				</div>
			{:else if questLinkMessage}
				<div class="flex items-center gap-2 shrink-0">
					<span class="text-xs text-white/50">Quest:</span>
					<span class="text-sm text-white/70">{questLinkMessage}</span>
					<Button variant="primary" size="sm" onclick={onDismissQuestLinkMessage}>Done</Button>
				</div>
			{/if}

			<!-- Armour cost remains reachable post-session for end-of-session bookkeeping. -->
			<div class="flex flex-col shrink-0 border-l border-white/10 pl-3">
				<div class="flex items-center gap-2 shrink-0">
					<span class="text-white/40 shrink-0">{@html ICON_ARMOUR}</span>
					<button
						bind:this={postSessionArmourButton}
						class="px-2 py-0.5 rounded-[4px] border text-[9px] font-medium transition-all cursor-pointer
							{armourCostOpen
								? 'bg-accent/20 border-accent/40 text-accent'
								: 'bg-white/5 border-white/10 text-white/60 hover:bg-white/10 hover:text-white/90'}"
						aria-haspopup="dialog"
						aria-expanded={armourCostOpen}
						onclick={onArmourCostToggle}
						title="Record armour cost"
					>
						Cost
					</button>
				</div>
				{#if armourCostError && !armourCostOpen}
					<div class="mt-1 px-1 text-[10px] leading-tight text-orange-300/90">
						{armourCostError}
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.overlay-strip {
		overflow: visible;
	}

	.glass-panel {
		background: rgba(10, 14, 23, 0.85);
		backdrop-filter: blur(16px) saturate(150%);
		border: 1px solid rgba(255, 255, 255, 0.08);
	}

	.release-btn {
		width: 18px;
		height: 18px;
		border-radius: 4px;
		border: 1px solid rgba(255, 255, 255, 0.15);
		background: rgba(255, 255, 255, 0.05);
		color: rgba(255, 255, 255, 0.4);
		font-size: 10px;
		line-height: 1;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 150ms ease-out;
	}
	.release-btn:hover {
		background: rgba(255, 255, 255, 0.1);
		color: rgba(255, 255, 255, 0.7);
		border-color: rgba(255, 255, 255, 0.25);
	}
	.release-btn:disabled {
		opacity: 0.3;
		cursor: default;
	}

	.start-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		border-radius: 5px;
		border: 1px solid rgba(52, 211, 153, 0.3);
		background: rgba(52, 211, 153, 0.1);
		color: rgba(52, 211, 153, 0.9);
		font-size: 11px;
		font-weight: 500;
		cursor: pointer;
		transition: all 150ms ease-out;
	}
	.start-btn:hover {
		background: rgba(52, 211, 153, 0.2);
		border-color: rgba(52, 211, 153, 0.5);
	}
	.start-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.stop-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border-radius: 4px;
		border: 1px solid rgba(248, 113, 113, 0.3);
		background: rgba(248, 113, 113, 0.1);
		color: rgba(248, 113, 113, 0.8);
		cursor: pointer;
		transition: all 150ms ease-out;
	}
	.stop-btn:hover {
		background: rgba(248, 113, 113, 0.2);
		border-color: rgba(248, 113, 113, 0.5);
		color: rgba(248, 113, 113, 1);
	}
	.stop-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.armour-prompt {
		padding: 3px 8px;
		border-radius: 5px;
		border: 1px solid rgba(251, 191, 36, 0.4);
		background: rgba(251, 191, 36, 0.12);
	}
	.armour-prompt-btn {
		padding: 2px 8px;
		border-radius: 4px;
		font-size: 10px;
		font-weight: 600;
		line-height: 1;
		border: 1px solid transparent;
		cursor: pointer;
		transition: background 120ms ease, border-color 120ms ease;
	}
	.armour-prompt-yes {
		background: rgba(251, 191, 36, 0.22);
		border-color: rgba(251, 191, 36, 0.5);
		color: rgb(253, 224, 71);
	}
	.armour-prompt-yes:hover {
		background: rgba(251, 191, 36, 0.32);
		border-color: rgba(251, 191, 36, 0.7);
	}
	.armour-prompt-no {
		background: rgba(255, 255, 255, 0.06);
		border-color: rgba(255, 255, 255, 0.18);
		color: rgba(255, 255, 255, 0.75);
	}
	.armour-prompt-no:hover {
		background: rgba(255, 255, 255, 0.12);
		border-color: rgba(255, 255, 255, 0.3);
	}
	.armour-prompt-close {
		padding: 0 6px;
		min-width: 18px;
		background: transparent;
		border-color: rgba(251, 191, 36, 0.35);
		color: rgba(251, 191, 36, 0.85);
		font-size: 13px;
		line-height: 1;
	}
	.armour-prompt-close:hover {
		background: rgba(251, 191, 36, 0.15);
		border-color: rgba(251, 191, 36, 0.6);
		color: rgb(253, 224, 71);
	}
	.armour-prompt-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
