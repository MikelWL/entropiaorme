<script lang="ts">
	import type { OverlayMenuSelection, OverlayMenuState } from '$lib/windows/overlayMenu';

	let {
		state,
		onSelect,
		onFocusSelect
	}: {
		state: OverlayMenuState;
		/** A pick that closes the menu (every kind but a focus quest toggle). */
		onSelect: (selection: OverlayMenuSelection) => void;
		/** A focus quest toggle, which keeps the picker open (the overlay
		 * re-shows it with the refreshed focused states), so joining a second
		 * daily is not a close-and-reopen. */
		onFocusSelect: (selection: OverlayMenuSelection) => void;
	} = $props();
</script>

<div class="menu-panel">
	{#if state.kind === 'focus'}
		{#if state.quests.length === 0 && state.presets.length === 0}
			<div class="menu-empty">No quests to focus</div>
		{:else}
			{@const anyFocused = state.quests.some((quest) => quest.focused)}
			{#each state.quests as quest (quest.questId)}
				<div class="menu-row">
					<button
						type="button"
						class="menu-option {quest.focused ? 'menu-option-active' : ''}"
						title={quest.focused
							? 'Unfocus (the stretch ends here)'
							: quest.signalQuest
								? 'Start a run: play counts toward it until its signal loot drops'
								: 'Focus: play from now on counts toward this quest'}
						onclick={() =>
							onFocusSelect(
								quest.focused
									? { kind: 'focus', action: 'questUnfocus', questId: quest.questId }
									: { kind: 'focus', action: 'questFocus', questId: quest.questId, additive: false }
							)}
					>
						<span class="menu-option-name">{quest.name}</span>
						{#if quest.focused}
							<span class="menu-option-badge">Focused</span>
						{:else if quest.signalQuest}
							<!-- The standing, repeatable chip: focusing starts a
								 run; the signal loot ends it. -->
							<span class="menu-option-badge menu-option-badge-muted">Run</span>
						{/if}
					</button>
					{#if !quest.focused && anyFocused}
						<button
							type="button"
							class="menu-join-btn"
							aria-label={`Also focus ${quest.name}`}
							title="Also focus: the play ahead advances this quest too"
							onclick={() =>
								onFocusSelect({
									kind: 'focus',
									action: 'questFocus',
									questId: quest.questId,
									additive: true
								})}
						>+</button>
					{/if}
				</div>
			{/each}
			{#if state.presets.length > 0}
				<div class="menu-heading">Recent segments</div>
				{#each state.presets as label (label)}
					<button
						type="button"
						class="menu-option"
						title="Start a segment with this name (closes the current one)"
						onclick={() => onSelect({ kind: 'focus', action: 'preset', label })}
					>
						<span class="menu-option-name">{label}</span>
					</button>
				{/each}
			{/if}
		{/if}
	{:else if state.kind === 'trifecta'}
		{#each state.options as option}
			<button
				type="button"
				class="menu-option {option.active ? 'menu-option-active' : ''}"
				onclick={() => onSelect({ kind: 'trifecta', presetId: option.id })}
			>
				<span class="menu-option-name">{option.name}</span>
				{#if option.active}
					<span class="menu-option-badge">Active</span>
				{/if}
			</button>
		{/each}
	{:else if state.loading}
		<div class="menu-empty">Searching...</div>
	{:else if state.error}
		<div class="menu-empty">{state.error}</div>
	{:else if state.kind === 'name'}
		{#if state.suggestions.length === 0}
			{@const typed = state.query.trim()}
			<button
				type="button"
				class="menu-option"
				onclick={() => onSelect({ kind: 'name', name: typed })}
			>
				<span class="menu-option-name">Press Enter to name it "{typed}"</span>
			</button>
		{:else}
			{#each state.suggestions as option}
				<button
					type="button"
					class="menu-option"
					onclick={() => onSelect({ kind: 'name', name: option })}
				>
					<span class="menu-option-name">{option}</span>
				</button>
			{/each}
		{/if}
	{:else if state.kind === 'mob'}
		{#if state.mobSuggestions.length === 0}
			<div class="menu-empty">No matches</div>
		{:else}
			{#each state.mobSuggestions as option}
				<button
					type="button"
					class="menu-option"
					onclick={() => onSelect({ kind: 'mob', species: option.species, maturity: option.maturity })}
				>
					<span class="menu-option-name">{option.display}</span>
				</button>
			{/each}
		{/if}
	{/if}
</div>

<style>
	.menu-panel {
		display: flex;
		flex-direction: column;
		width: 100%;
		max-height: 220px;
		overflow-y: auto;
		padding: 6px;
		border-radius: 8px;
		border: 1px solid rgba(255, 255, 255, 0.12);
		background: rgba(11, 15, 25, 0.96);
		backdrop-filter: blur(16px) saturate(150%);
		box-shadow:
			0 14px 30px rgba(0, 0, 0, 0.48),
			0 0 0 1px rgba(255, 255, 255, 0.03);
	}

	.menu-option {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		width: 100%;
		padding: 7px 8px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: rgba(255, 255, 255, 0.82);
		font-size: 12px;
		text-align: left;
		cursor: pointer;
		transition:
			background-color 140ms ease,
			color 140ms ease;
	}

	.menu-option:hover,
	.menu-option:focus-visible {
		background: rgba(255, 255, 255, 0.06);
		color: rgba(255, 255, 255, 0.94);
		outline: none;
	}

	.menu-option-active {
		background: rgba(56, 189, 248, 0.14);
		color: rgba(186, 230, 253, 0.98);
	}

	.menu-option-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.menu-option-badge {
		flex-shrink: 0;
		padding: 2px 6px;
		border-radius: 999px;
		background: rgba(56, 189, 248, 0.16);
		color: rgba(186, 230, 253, 0.96);
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	.menu-option-badge-muted {
		background: rgba(255, 255, 255, 0.07);
		color: rgba(255, 255, 255, 0.45);
	}

	.menu-empty {
		padding: 8px 10px;
		color: rgba(255, 255, 255, 0.45);
		font-size: 11px;
	}

	.menu-row {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.menu-row .menu-option {
		flex: 1;
		min-width: 0;
	}

	.menu-join-btn {
		flex-shrink: 0;
		width: 22px;
		height: 22px;
		border-radius: 5px;
		border: 1px solid rgba(255, 255, 255, 0.15);
		background: rgba(255, 255, 255, 0.05);
		color: rgba(255, 255, 255, 0.5);
		font-size: 12px;
		line-height: 1;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 140ms ease;
	}
	.menu-join-btn:hover,
	.menu-join-btn:focus-visible {
		background: rgba(56, 189, 248, 0.16);
		border-color: rgba(56, 189, 248, 0.4);
		color: rgba(186, 230, 253, 0.96);
		outline: none;
	}

	.menu-heading {
		padding: 8px 8px 3px;
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: rgba(255, 255, 255, 0.3);
	}
</style>
