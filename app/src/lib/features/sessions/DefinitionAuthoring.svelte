<script lang="ts">
	import { untrack } from 'svelte';
	import { quintOut } from 'svelte/easing';
	import { Button, ErrorNotice, SearchInput, Select, Toggle } from '$lib/components';
	import { InDevelopmentMark, inDevelopment } from '$lib/inDevelopment';
	import { shouldSettleInstantly } from '$lib/motion/testMotion';
	import type { DefinitionsModel, RosterDraftEntry } from './definitionsModel.svelte';

	let {
		model
	}: {
		model: DefinitionsModel;
	} = $props();

	let nameInput = $state<HTMLInputElement | null>(null);
	let sourceFilter = $state('');
	let segmentDraft = $state('');

	// Activities are progressive disclosure: a session is worth saving on
	// its name alone, so the roster stays folded away until asked for.
	// Inside it the catalogue is reached through its planet, which is how
	// the Quests page scopes the same content.
	let activitiesOpen = $state(false);
	let planet = $state<string | null>(null);

	const editing = $derived(model.mode === 'edit');

	/** The planets that actually have something to offer, so choosing one
	 * can never lead to an empty list. */
	const activityPlanets = $derived(
		[
			...new Set([
				...model.families.map((family) => family.planet),
				...model.quests.map((quest) => quest.planet)
			])
		].sort()
	);

	function matchesFilter(name: string): boolean {
		return name.toLowerCase().includes(sourceFilter.trim().toLowerCase());
	}

	const filteredFamilies = $derived(
		planet === null
			? []
			: model.families.filter((family) => family.planet === planet && matchesFilter(family.name))
	);
	const filteredQuests = $derived(
		planet === null
			? []
			: model.quests.filter((quest) => quest.planet === planet && matchesFilter(quest.name))
	);

	function inRoster(kind: 'quest_family' | 'quest', refId: string): boolean {
		return model.roster.some((entry) => entry.kind === kind && entry.refId === refId);
	}

	function kindLabel(entry: RosterDraftEntry): string {
		return entry.kind === 'quest_family' ? 'Family' : entry.kind === 'quest' ? 'Quest' : 'Segment';
	}

	function addSegmentDraft() {
		model.addSegment(segmentDraft);
		segmentDraft = '';
	}

	/** The surface's entrance: it arrives AFTER the page content has
	 * animated away (the stage sequences that with a matching delay on
	 * its content wrapper), fading in with a slight upward settle; on
	 * close it leaves first, undelayed, and the page returns behind it.
	 * Reduced motion and the e2e freeze collapse both to an instant
	 * settle. */
	function surface(_node: HTMLElement, { entering }: { entering: boolean }) {
		if (shouldSettleInstantly()) return { duration: 0 };
		return {
			duration: entering ? 260 : 170,
			delay: entering ? 160 : 0,
			easing: quintOut,
			css: (t: number, u: number) => `opacity: ${t}; transform: translateY(${12 * u}px);`
		};
	}

	// Escape leaves without saving. Tab is deliberately NOT trapped: the
	// environment takes over the page's own region, not the window, so the
	// sidebar and titlebar stay reachable and navigating away is a valid
	// way out of it.
	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') model.close();
	}

	// On open: remember the opener and move focus to the name field; on
	// close hand focus back. The disclosure starts open only when there is
	// something authored to see, and the planet choice starts fresh
	// (untracked: neither should move again while the editor is open).
	$effect(() => {
		if (model.mode === 'closed') return;
		void model.editingId;
		untrack(() => {
			activitiesOpen = model.roster.length > 0 || model.adHocSegments;
			planet = null;
			sourceFilter = '';
		});
		const previouslyFocused =
			document.activeElement instanceof HTMLElement ? document.activeElement : null;
		nameInput?.focus();
		return () => {
			previouslyFocused?.focus();
		};
	});
</script>

<svelte:window onkeydown={model.mode !== 'closed' ? handleKeydown : undefined} />

{#if model.mode !== 'closed'}
	<div
		class="absolute inset-0 z-30 overflow-y-auto bg-base focus:outline-hidden"
		role="region"
		aria-label={editing ? 'Edit session' : 'New session'}
		tabindex="-1"
		in:surface={{ entering: true }}
		out:surface={{ entering: false }}
	>
		<div class="mx-auto flex min-h-full w-full max-w-2xl flex-col gap-6 px-6 py-10">
			<!-- Header: identity + the exits -->
			<div class="flex items-start justify-between gap-4">
				<h2 class="text-2xl font-semibold tracking-tight text-text">
					{editing ? 'Edit session' : 'New session'}
				</h2>
				<button
					class="h-8 w-8 flex items-center justify-center rounded-md text-text-secondary
						cursor-pointer transition-colors duration-[var(--duration-fast)]
						hover:text-text hover:bg-surface-hover shrink-0"
					onclick={() => model.close()}
					aria-label="Close without saving"
					title="Close without saving (Esc)"
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-4.5 w-4.5">
						<path
							d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z"
						/>
					</svg>
				</button>
			</div>

			<!-- Name: the session's identity, so it gets the headline slot -->
			<input
				bind:this={nameInput}
				bind:value={model.name}
				class="w-full bg-transparent border-b border-border focus:border-accent text-xl
					font-semibold tracking-tight text-text px-1 py-2 outline-none
					placeholder:text-text-tertiary transition-colors"
				placeholder="Name this session..."
				aria-label="Session name"
				disabled={model.saving}
			/>

			<!-- Activities: the roster and its on-the-fly option, folded away
				 behind one disclosure. Authored and saved now, consumed by the
				 overlay's roster-fed activity picker once that control lands.
				 Registered in-development: hidden on the stable channel, marked
				 everywhere else. -->
			{#if inDevelopment.visible}
			<section class="panel flex flex-col">
				<div class="flex items-center gap-2 pr-4">
					<button
						type="button"
						class="flex flex-1 items-center gap-2.5 px-4 py-3 text-left cursor-pointer
							transition-colors duration-[var(--duration-base)] hover:bg-surface-hover/40"
						aria-expanded={activitiesOpen}
						aria-controls="session-activities"
						onclick={() => (activitiesOpen = !activitiesOpen)}
					>
						<svg
							class="h-3.5 w-3.5 shrink-0 text-text-secondary transition-transform
								{activitiesOpen ? '' : '-rotate-90'}"
							fill="none"
							stroke="currentColor"
							viewBox="0 0 24 24"
							aria-hidden="true"
						>
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
						</svg>
						<span class="text-sm font-semibold text-text">Activities</span>
						{#if model.roster.length > 0}
							<span class="text-sm text-text-secondary tabular-nums">{model.roster.length}</span>
						{/if}
					</button>
					<InDevelopmentMark id="session-definition-roster" />
				</div>

				{#if activitiesOpen}
					<div id="session-activities" class="flex flex-col gap-3 border-t border-border/50 p-4">
						{#if model.roster.length > 0}
							<ul class="flex flex-col gap-1">
								{#each model.roster as entry, i (`${entry.kind}:${entry.refId ?? entry.label}:${i}`)}
									<li
										class="flex items-center gap-2 rounded-md border border-border/60 bg-base/40 px-3 py-2"
									>
										<span class="eyebrow-strong w-[4.5rem] shrink-0">{kindLabel(entry)}</span>
										{#if entry.missing}
											<span class="text-sm text-warning truncate flex-1" title="The referenced item was deleted; this entry is dropped on save">
												{entry.displayName} (removed)
											</span>
										{:else}
											<span class="text-sm text-text truncate flex-1">{entry.displayName}</span>
										{/if}
										<div class="flex items-center gap-0.5 shrink-0">
											<button
												type="button"
												class="p-1 text-text-secondary cursor-pointer transition-colors
													duration-[var(--duration-base)] hover:text-text
													disabled:opacity-40 disabled:cursor-not-allowed"
												aria-label="Move up"
												title="Move up"
												disabled={i === 0 || model.saving}
												onclick={() => model.moveEntry(i, -1)}
											>&uarr;</button>
											<button
												type="button"
												class="p-1 text-text-secondary cursor-pointer transition-colors
													duration-[var(--duration-base)] hover:text-text
													disabled:opacity-40 disabled:cursor-not-allowed"
												aria-label="Move down"
												title="Move down"
												disabled={i === model.roster.length - 1 || model.saving}
												onclick={() => model.moveEntry(i, 1)}
											>&darr;</button>
											<button
												type="button"
												class="icon-button-row p-1"
												aria-label="Remove from the session"
												title="Remove"
												disabled={model.saving}
												onclick={() => model.removeEntry(i)}
											>&times;</button>
										</div>
									</li>
								{/each}
							</ul>
						{/if}

						<!-- The catalogue, reached through its planet: pick one, then
							 what that planet offers appears. -->
						{#if activityPlanets.length > 0}
							<div class="flex items-center gap-2">
								<label for="session-activity-planet" class="eyebrow-strong shrink-0">Planet</label>
								<Select
									id="session-activity-planet"
									class="max-w-[14rem]"
									bind:value={planet}
									disabled={model.saving}
								>
									<option value={null}>Choose a planet</option>
									{#each activityPlanets as option (option)}
										<option value={option}>{option}</option>
									{/each}
								</Select>
							</div>
						{:else if !model.sourcesLoading}
							<p class="text-sm text-text-secondary px-1">
								No quests or quest families yet; they are authored on the Quests page.
							</p>
						{/if}

						{#if planet !== null}
							<SearchInput
								bind:value={sourceFilter}
								placeholder="Filter {planet}..."
								loading={model.sourcesLoading}
							/>
							{#if filteredFamilies.length > 0}
								<div class="flex flex-col gap-1.5">
									<span class="eyebrow-strong">Quest families</span>
									<div class="flex flex-wrap gap-1.5">
										{#each filteredFamilies as fam (fam.id)}
											<button
												class="filter-chip"
												disabled={inRoster('quest_family', fam.id) || model.saving}
												title={inRoster('quest_family', fam.id)
													? 'Already in this session'
													: 'Add to this session'}
												onclick={() => model.addFamily(fam)}
											>
												{fam.name}
											</button>
										{/each}
									</div>
								</div>
							{/if}
							{#if filteredQuests.length > 0}
								<div class="flex flex-col gap-1.5">
									<span class="eyebrow-strong">Quests</span>
									<div class="flex flex-wrap gap-1.5">
										{#each filteredQuests as quest (quest.id)}
											<button
												class="filter-chip"
												disabled={inRoster('quest', quest.id) || model.saving}
												title={inRoster('quest', quest.id)
													? 'Already in this session'
													: 'Add to this session'}
												onclick={() => model.addQuest(quest)}
											>
												{quest.name}
											</button>
										{/each}
									</div>
								</div>
							{/if}
							{#if !model.sourcesLoading && filteredFamilies.length === 0 && filteredQuests.length === 0}
								<p class="text-sm text-text-secondary px-1">
									{sourceFilter.trim()
										? 'Nothing matches the filter.'
										: `Nothing on ${planet} yet; quests are authored on the Quests page.`}
								</p>
							{/if}
						{/if}

						<!-- Segments are the activities the player names themselves, so
							 they are one capability with one switch: off, the session has
							 no segment concept at all; on, names can be seeded here as
							 well as typed during play. -->
						<div class="flex items-center justify-between gap-4 border-t border-border/50 pt-3">
							<div class="flex flex-col gap-0.5">
								<span class="text-sm text-text">Name segments on the fly</span>
								<span class="text-sm text-text-secondary leading-relaxed max-w-sm">
									Type a segment name while you play, instead of only picking from this list.
								</span>
							</div>
							<Toggle
								checked={model.adHocSegments}
								disabled={model.saving}
								label="Name segments on the fly"
								onchange={(checked) => (model.adHocSegments = checked)}
							/>
						</div>

						{#if model.adHocSegments}
							<div class="flex items-center gap-2">
								<input
									bind:value={segmentDraft}
									class="flex-1 h-9 px-3 text-sm bg-surface/70 text-text rounded-md border border-border
										outline-none transition-colors focus:border-accent/60
										placeholder:text-text-tertiary"
									placeholder="Name one now if you already know it (e.g. Warm-up)..."
									aria-label="New segment name"
									disabled={model.saving}
									onkeydown={(e) => {
										if (e.key === 'Enter') {
											e.preventDefault();
											addSegmentDraft();
										}
									}}
								/>
								<Button size="sm" variant="secondary" disabled={model.saving || !segmentDraft.trim()} onclick={addSegmentDraft}>
									{#snippet children()}Add segment{/snippet}
								</Button>
							</div>
						{/if}
					</div>
				{/if}
			</section>
			{/if}

			{#if model.authoringError}
				<ErrorNotice message={model.authoringError} />
			{/if}

			<!-- Footer: destructive on the left, the exits on the right -->
			<div class="mt-auto flex items-center justify-between gap-3 pt-2">
				<div>
					{#if editing}
						<Button
							size="sm"
							variant="danger"
							disabled={model.saving}
							onclick={() => model.deleteEditing()}
						>
							{#snippet children()}{model.deleteArmed ? 'Really delete?' : 'Delete'}{/snippet}
						</Button>
					{/if}
				</div>
				<div class="flex items-center gap-2">
					<Button size="sm" variant="secondary" disabled={model.saving} onclick={() => model.close()}>
						{#snippet children()}Cancel{/snippet}
					</Button>
					<Button size="sm" disabled={model.saving || !model.name.trim()} onclick={() => model.save()}>
						{#snippet children()}{model.saving ? 'Saving...' : editing ? 'Save changes' : 'Create'}{/snippet}
					</Button>
				</div>
			</div>
		</div>
	</div>
{/if}
