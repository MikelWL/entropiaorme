<script lang="ts">
	import type { TrackingLive, TrackingStatus } from '$lib/api';
	import { overlayStats } from '$lib/statsCustomisation.svelte';
	import { getStatDef } from '$lib/statsRegistry';
	import TrifectaSelector from './TrifectaSelector.svelte';
	import { ICON_EQUIPMENT, ICON_ARMOUR } from './icons';
	import { NO_DATA } from '$lib/utils/format';

	type LastSessionStats = { cost: number; returns: number; pes: number; net: number };

	const noop = () => {};

	let {
		data,
		status = null,
		toggling = false,
		releasing = false,
		selectingMob = false,
		savingDefinition = false,
		definitionEditable = true,
		savingBoost = false,
		savingSegment = false,
		savingFocus = false,
		focusMenuOpen = false,
		facetError = null,
		trifectaSaving = false,
		trifectaError = null,
		armourCostOpen = false,
		armourCostError = null,
		armourSessionId = null,
		mobMenuOpen = false,
		definitionMenuOpen = false,
		trifectaMenuOpen = false,
		overlayMenuLaunchError = null,
		lastSessionId = null,
		lastSessionStats = null,
		mobQuery = $bindable(''),
		mobInput = $bindable(null),
		boostDraft = $bindable(''),
		segmentDraft = $bindable(''),
		postSessionArmourButton = $bindable(null),
		awaitingArmourTrackDecision = false,
		attributionWarning = null,
		onStart = noop,
		onStop = noop,
		onArmourTrackDecision = noop,
		onDismissAttributionWarning = noop,
		onReleaseMob = noop,
		onMobFocus = noop,
		onMobBlur = noop,
		onMobKeydown = noop,
		onDefinitionTrigger = noop,
		onClearDefinition = noop,
		onBoostCommit = noop,
		onSegmentCommit = noop,
		onSegmentBlur = noop,
		onSegmentNext = noop,
		onSegmentClose = noop,
		onFocusTrigger = noop,
		onTrifectaTrigger = noop,
		onArmourCostToggle = noop
	}: {
		data: TrackingLive;
		status?: TrackingStatus | null;
		toggling?: boolean;
		releasing?: boolean;
		selectingMob?: boolean;
		savingDefinition?: boolean;
		definitionEditable?: boolean;
		savingBoost?: boolean;
		savingSegment?: boolean;
		savingFocus?: boolean;
		focusMenuOpen?: boolean;
		facetError?: string | null;
		trifectaSaving?: boolean;
		trifectaError?: string | null;
		armourCostOpen?: boolean;
		armourCostError?: string | null;
		armourSessionId?: string | null;
		mobMenuOpen?: boolean;
		definitionMenuOpen?: boolean;
		trifectaMenuOpen?: boolean;
		overlayMenuLaunchError?: string | null;
		lastSessionId?: string | null;
		lastSessionStats?: LastSessionStats | null;
		mobQuery?: string;
		mobInput?: HTMLInputElement | null;
		boostDraft?: string;
		segmentDraft?: string;
		postSessionArmourButton?: HTMLButtonElement | null;
		awaitingArmourTrackDecision?: boolean;
		attributionWarning?: string | null;
		onStart?: () => void | Promise<void>;
		onStop?: () => void | Promise<void>;
		onArmourTrackDecision?: (action: 'yes' | 'no') => void | Promise<void>;
		onDismissAttributionWarning?: () => void;
		onReleaseMob?: () => void | Promise<void>;
		onMobFocus?: () => void;
		onMobBlur?: () => void;
		onMobKeydown?: (event: KeyboardEvent) => void | Promise<void>;
		onDefinitionTrigger?: (anchor: HTMLButtonElement) => void | Promise<void>;
		onClearDefinition?: () => void | Promise<void>;
		onBoostCommit?: () => void | Promise<void>;
		onSegmentCommit?: () => void | Promise<void>;
		onSegmentBlur?: () => void | Promise<void>;
		onSegmentNext?: () => void | Promise<void>;
		onSegmentClose?: () => void | Promise<void>;
		onFocusTrigger?: (anchor: HTMLButtonElement) => void | Promise<void>;
		onTrifectaTrigger?: (anchor: HTMLButtonElement) => void | Promise<void>;
		onArmourCostToggle?: (event: MouseEvent) => void | Promise<void>;
	} = $props();

	const isTrifectaAttribution = $derived(data.weaponAttribution === 'trifecta');
	const isActive = $derived(data.status === 'active');
	// The declared mob is the kill-stamp source and may change mid-session,
	// so its input is available in both states; a standing declaration
	// shows as a label with a release control beside it.
	const showManualInput = $derived(
		(data.status === 'active' || data.status === 'idle') && !data.currentMob
	);
	// The session (and the name it writes) is session-grain: picked
	// before a session, fixed while one runs, corrected afterwards on the
	// session record. (The boost is the other way round: it stamps each
	// skill gain, so it stays editable throughout.)
	// The quest facet's readout: auto-recorded open stretches, never
	// declared here (the quest lifecycle owns the writes).
	const questNames = $derived(data.questNames ?? null);
	// What the held tool implies the next action records as. Derived from
	// evidence, never declared, so it is shown as feedback and never asked.
	const activityLabel = $derived(
		data.currentActivity === 'treecutting'
			? 'Tree Cutting'
			: data.currentActivity === 'hunting'
				? 'Hunting'
				: null
	);

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

		<!-- Session facets: the independent, co-recorded attributions a
			 session carries. Each control here declares gameplay from now on,
			 so a facet is editable while a session runs only if its stamp is
			 finer-grained than the session. The boost is (it stamps each skill
			 gain, so a pill expiring is recordable); the name is not (it names
			 the whole session, so a live edit could only rewrite history) and
			 is corrected on the session record instead. -->
		<div
			class="flex items-center gap-2 shrink-0 border-r border-white/10 pr-3"
			data-guide-anchor="overlay-session-section"
		>
			<div class="w-32 flex flex-col shrink-0">
				<span class="facet-label">Session</span>
				<div class="flex items-center gap-1" data-testid="definition-facet">
					{#if isActive && data.sessionName}
						<div
							class="text-sm font-medium text-white/90 truncate px-1 min-w-0 flex-1"
							title={`${data.sessionName} (fixed for this session; correct it from the session record once it ends)`}
						>
							{data.sessionName}
						</div>
					{:else}
						<button
							type="button"
							class="facet-chip min-w-0 flex-1 {data.sessionName ? 'facet-chip-open' : ''}"
							disabled={savingDefinition || !definitionEditable}
							aria-haspopup="menu"
							aria-expanded={definitionMenuOpen}
							title={!definitionEditable
								? 'The session is fixed while one runs'
								: data.sessionName
									? `${data.sessionName}; pick the session for the next run`
									: 'Pick the session for the next run'}
							onclick={(event) => onDefinitionTrigger(event.currentTarget as HTMLButtonElement)}
						>
							{#if data.sessionName}
								<span class="truncate">{data.sessionName}</span>
							{:else}
								<span class="text-white/40">Pick...</span>
							{/if}
						</button>
						{#if data.sessionName}
							<button
								type="button"
								class="release-btn shrink-0"
								aria-label="Clear session"
								disabled={savingDefinition || !definitionEditable}
								onclick={onClearDefinition}
								title="Clear session"
							>
								{savingDefinition ? '...' : 'x'}
							</button>
						{/if}
					{/if}
				</div>
			</div>

			<!-- Skill boost: the labelled percentage of the pill in force,
				 because it changes how PES reads. Three declarations, not
				 two: blank claims nothing, a typed 0 declares deliberately
				 unboosted play (the baseline a boost's effect is measured
				 against), and a number declares its magnitude. Editable at
				 any time: re-declaring when a pill runs out marks every gain
				 from that moment onward, and never touches the ones already
				 stamped. -->
			<div class="flex flex-col shrink-0">
				<span class="facet-label">Boost</span>
				<div class="flex items-baseline">
					<input
						class="w-9 bg-transparent border-b border-white/10 focus:border-accent text-sm text-white/90 px-1 py-0.5 outline-none placeholder:text-white/20 tabular-nums transition-colors"
						bind:value={boostDraft}
						placeholder={NO_DATA}
						inputmode="numeric"
						aria-label="Skill boost percent"
						title="Boost percent in force. Leave blank to claim nothing; enter 0 to record deliberately unboosted play."
						disabled={savingBoost}
						onblur={onBoostCommit}
						onkeydown={(event) => {
							if (event.key === 'Enter') {
								event.preventDefault();
								void onBoostCommit();
							}
						}}
					/>
					<span class="text-[10px] text-white/30 leading-none">%</span>
				</div>
			</div>

			<!-- Quest: the focus declaration. The mission log only witnesses
				 pickup and hand-in, so which stretch of play advances a quest
				 is declared here: the picker lists the in-progress quests
				 (one tap switches, + joins additively) and recalls segment
				 presets for this session name. A completion closes its
				 focused stretch by itself. -->
			<div class="flex flex-col shrink-0">
				<span class="facet-label">Quest</span>
				<div class="flex items-center" data-testid="quest-facet">
					<button
						type="button"
						class="facet-chip {questNames && questNames.length > 0 ? 'facet-chip-open' : ''}"
						disabled={!isActive || savingFocus}
						aria-haspopup="menu"
						aria-expanded={focusMenuOpen}
						title={!isActive
							? 'Start a session to declare which quest the play is toward'
							: questNames && questNames.length > 0
								? `Focused: ${questNames.join(', ')}`
								: 'Declare which quest the play from now on is toward'}
						onclick={(event) => onFocusTrigger(event.currentTarget as HTMLButtonElement)}
					>
						{#if questNames && questNames.length > 0}
							<span class="truncate max-w-[110px]"
								>{questNames[0]}{questNames.length > 1 ? ` +${questNames.length - 1}` : ''}</span
							>
						{:else if data.questsInProgress}
							<span class="whitespace-nowrap"
								>{data.questsInProgress} ready</span
							>
						{:else}
							<span>{NO_DATA}</span>
						{/if}
					</button>
				</div>
			</div>

			<!-- Segment: the player-drawn slice of the session (one instance
				 run, one rotation of a daily). Finer-grained than the session,
				 so it edits live: the field renames the open segment in place,
				 and the + button draws the next boundary in one click, closing
				 the standing segment (segments are sequential). A blank name
				 is auto-numbered by open count. -->
			<div class="flex flex-col shrink-0">
				<span class="facet-label">Segment</span>
				<div class="flex items-center gap-1">
					<input
						class="w-24 bg-transparent border-b border-white/10 focus:border-accent text-sm text-white/90 px-1 py-0.5 outline-none placeholder:text-white/20 transition-colors"
						bind:value={segmentDraft}
						placeholder={isActive ? 'Segment...' : NO_DATA}
						aria-label="Segment name"
						title={!isActive
							? 'Start a session to draw segments'
							: data.segmentName
								? 'Rename the open segment'
								: 'Name the next segment; leave blank to auto-number'}
						disabled={!isActive || savingSegment}
						onblur={onSegmentBlur}
						onkeydown={(event) => {
							if (event.key === 'Enter') {
								event.preventDefault();
								void onSegmentCommit();
							}
						}}
					/>
					<button
						type="button"
						class="release-btn shrink-0"
						aria-label={data.segmentName ? 'Start next segment' : 'Start segment'}
						disabled={!isActive || savingSegment}
						onclick={onSegmentNext}
						title={data.segmentName
							? 'Start the next segment (closes the current one)'
							: 'Start a segment'}
					>+</button>
					{#if data.segmentName && isActive}
						<button
							type="button"
							class="release-btn shrink-0"
							aria-label="Close segment"
							disabled={savingSegment}
							onclick={onSegmentClose}
							title="Close the open segment"
						>x</button>
					{/if}
				</div>
			</div>
		</div>

		<!-- Declared mob: the source of each kill's mob stamp, changeable
			 mid-session (an off-declaration kill still stamps the declared
			 mob until detection can read the target directly). -->
		<div
			class="flex items-center gap-2 shrink-0 border-r border-white/10 pr-3"
			data-guide-anchor="overlay-mob-section"
		>
			<div class="w-32 flex flex-col shrink-0">
				<span class="facet-label">Mob</span>
				<div class="flex items-center">
					{#if showManualInput}
						<input
							bind:this={mobInput}
							class="w-full bg-transparent border-b border-white/10 focus:border-accent text-sm text-white/90 px-1 py-0.5 outline-none placeholder:text-white/20 transition-colors"
							bind:value={mobQuery}
							placeholder="Mob..."
							disabled={selectingMob}
							onfocus={onMobFocus}
							onblur={onMobBlur}
							onkeydown={onMobKeydown}
						/>
					{:else if data.currentMob}
						<div class="text-sm font-medium text-white/90 truncate px-1 w-full">{data.currentMob}</div>
					{:else}
						<div class="text-sm font-medium text-white/20 px-1">{NO_DATA}</div>
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
					title="Release mob"
				>
					{releasing ? '...' : 'x'}
				</button>
			{/if}
		</div>

		{#if facetError}
			<div class="shrink-0 max-w-[180px] text-[10px] leading-tight text-orange-300/90 border-r border-white/10 pr-3">
				{facetError}
			</div>
		{/if}

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
			{:else if data.harvestGuardrail}
				<!-- The guardrail cue: loot evidence disagrees with the hotbar's
					 tool. The believed tool shows in red (the questionable
					 belief) with the corrected attribution beneath, until a
					 hotbar press or agreeing loot resolves it. -->
				<div
					class="flex flex-col min-w-0"
					title={`Board output says ${data.harvestGuardrail.expectedTool}; hotbar shows ${data.harvestGuardrail.observedTool ?? 'no tool'}`}
					data-testid="guardrail-alert"
				>
					<div class="text-xs text-red-400 animate-pulse truncate max-w-[120px]">
						{data.harvestGuardrail.observedTool ?? 'No tool'}
					</div>
					<!-- Never truncated: what is actually being recorded must be
						 readable in full, so the self-sizing window widens for it. -->
					<div class="text-[10px] leading-tight text-white/70 whitespace-nowrap">
						Recording: {data.harvestGuardrail.expectedTool}
					</div>
				</div>
			{:else}
				<div class="flex flex-col min-w-0">
					<div class="text-xs {data.currentTool ? 'text-white/70' : 'text-white/20'} truncate max-w-[120px]">
						{data.currentTool || NO_DATA}
					</div>
					{#if activityLabel}
						<!-- Derived, never declared: the held tool implies which
							 activity the next action records as. What actually gets
							 recorded still follows the loot evidence, so this reads
							 as feedback the user can catch disagreeing. -->
						<div
							class="text-[9px] leading-tight uppercase tracking-wider text-white/35 whitespace-nowrap"
							title="Held tool: recording as {activityLabel}"
							data-testid="activity-feedback"
						>
							{activityLabel}
						</div>
					{/if}
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

		<!-- Customisable stat pills (driven by the overlay stat prefs): treated as
			 one unit, so the section separator sits at the unit boundary, not
			 between individual pills. -->
		{@const enabledPills = overlayStats.current.filter((p) => p.enabled)}
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
							<span class="text-[10px] font-bold text-white/40 tracking-wider uppercase leading-none">{def.shortLabel ?? def.label}</span>
							<span class="text-sm font-semibold tabular-nums leading-none {valueColor}">{r.value}</span>
						</div>
					{/if}
				{/each}
			</div>
		{/if}
	{:else}
		<!-- Post-session readout: the stopped session's final totals, held
			 while the armour-cost popup needs them. -->
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

	.facet-label {
		font-size: 9px;
		font-weight: 700;
		line-height: 1;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: rgba(255, 255, 255, 0.3);
		padding-left: 4px;
		margin-bottom: 2px;
	}

	.facet-chip {
		display: flex;
		align-items: center;
		padding: 2px 8px;
		border-radius: 4px;
		border: 1px solid rgba(255, 255, 255, 0.1);
		background: rgba(255, 255, 255, 0.05);
		color: rgba(255, 255, 255, 0.7);
		font-size: 11px;
		line-height: 1.35;
		cursor: pointer;
		transition: all 150ms ease-out;
	}
	.facet-chip:hover:not(:disabled) {
		background: rgba(255, 255, 255, 0.1);
		color: rgba(255, 255, 255, 0.9);
		border-color: rgba(255, 255, 255, 0.25);
	}
	.facet-chip-open {
		background: rgba(56, 189, 248, 0.2);
		border-color: rgba(56, 189, 248, 0.4);
		color: rgb(125, 211, 252);
	}
	.facet-chip:disabled {
		opacity: 0.35;
		cursor: default;
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
