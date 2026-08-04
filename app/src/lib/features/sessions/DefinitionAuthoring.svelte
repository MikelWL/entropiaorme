<script lang="ts">
	import { quintOut } from 'svelte/easing';
	import { Button, ErrorNotice, SearchInput, Toggle } from '$lib/components';
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

	const editing = $derived(model.mode === 'edit');

	const filteredFamilies = $derived(
		model.families.filter((family) =>
			family.name.toLowerCase().includes(sourceFilter.trim().toLowerCase())
		)
	);
	const filteredQuests = $derived(
		model.quests.filter((quest) =>
			quest.name.toLowerCase().includes(sourceFilter.trim().toLowerCase())
		)
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
	// close hand focus back.
	$effect(() => {
		if (model.mode === 'closed') return;
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

			<!-- Roster + ad-hoc preference: authored and saved now, consumed
				 by the overlay's roster-fed activity picker once that control
				 lands. Registered in-development: hidden on the stable
				 channel, marked everywhere else. -->
			{#if inDevelopment.visible}
			<section class="panel p-4 flex flex-col gap-3">
				<div class="flex items-baseline justify-between gap-2">
					<span class="eyebrow-strong">Roster</span>
					<InDevelopmentMark id="session-definition-roster" />
				</div>

				{#if model.roster.length === 0}
					<p class="text-sm text-text-secondary px-1 py-2">
						No activities yet. Add quest families, quests, or segment labels below.
					</p>
				{:else}
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
										aria-label="Remove from roster"
										title="Remove"
										disabled={model.saving}
										onclick={() => model.removeEntry(i)}
									>&times;</button>
								</div>
							</li>
						{/each}
					</ul>
				{/if}

				<!-- Add sources: the authored catalogue, filtered -->
				<div class="flex flex-col gap-2 border-t border-border/50 pt-3">
					<SearchInput
						bind:value={sourceFilter}
						placeholder="Filter quest families and quests..."
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
											? 'Already on the roster'
											: 'Add to the roster'}
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
										title={inRoster('quest', quest.id) ? 'Already on the roster' : 'Add to the roster'}
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
								: 'No quest families or quests yet; they are authored on the Quests page.'}
						</p>
					{/if}
					<div class="flex items-center gap-2 pt-1">
						<input
							bind:value={segmentDraft}
							class="flex-1 h-9 px-3 text-sm bg-surface/70 text-text rounded-md border border-border
								outline-none transition-colors focus:border-accent/60
								placeholder:text-text-tertiary"
							placeholder="Add a segment label (e.g. Warm-up)..."
							aria-label="New segment label"
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
				</div>
			</section>

			<!-- Options -->
			<section class="panel p-4 flex items-center justify-between gap-4">
				<div class="flex flex-col gap-0.5">
					<span class="text-sm text-text">Ad-hoc segments</span>
					<span class="text-sm text-text-secondary leading-relaxed max-w-sm">
						Allow improvised free-text segment names for this session; off, it relies on
						its roster's authored labels.
					</span>
				</div>
				<Toggle
					checked={model.adHocSegments}
					disabled={model.saving}
					label="Ad-hoc segments"
					onchange={(checked) => (model.adHocSegments = checked)}
				/>
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
