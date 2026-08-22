<script lang="ts">
	import {
		assignSessionProtectionLoadout,
		pendingProtectionAttribution,
		type PendingProtectionSession,
		type ProtectionOverview,
	} from '$lib/api';
	import Button from '$lib/components/Button.svelte';

	interface Props {
		sessionId: string;
		protection: ProtectionOverview | null;
		onassigned: (loadoutId: string) => void;
	}

	let { sessionId, protection, onassigned }: Props = $props();

	let pending = $state<PendingProtectionSession[] | null>(null);
	let assigningId = $state<string | null>(null);
	let error = $state<string | null>(null);
	let chosenHere = $state<string | null>(null);

	const thisSessionPending = $derived(
		pending?.find((session) => session.sessionId === sessionId) ?? null,
	);
	// The setup this session would record under: one named just now, or the
	// one already worn when the session was never owed a choice. Nothing
	// carries forward until one of those is a real setup, so recording can
	// never open against an armour setup that was never named.
	const carriedLoadoutId = $derived(
		chosenHere ?? (thisSessionPending === null ? (protection?.activeLoadoutId ?? null) : null),
	);
	const ready = $derived(pending !== null && carriedLoadoutId !== null);
	const others = $derived(pending?.filter((session) => session.sessionId !== sessionId) ?? []);

	async function load() {
		try {
			pending = await pendingProtectionAttribution();
			error = null;
		} catch (cause) {
			// Leaving the list at its last-good value: emptying it would make
			// this session look owed nothing and drop the others outright.
			error = cause instanceof Error ? cause.message : 'Sessions could not be read';
		}
	}

	$effect(() => {
		void load();
	});

	async function choose(select: HTMLSelectElement, target: string, loadoutId: string) {
		if (!protection || assigningId) return;
		assigningId = target;
		error = null;
		try {
			await assignSessionProtectionLoadout(target, loadoutId);
			if (target === sessionId) chosenHere = loadoutId;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Armour setup could not be saved';
			// Re-selecting the same option fires no change event, so a failure
			// that left the choice in place would be unretryable.
			select.value = '';
		} finally {
			assigningId = null;
		}
	}

	function whenLabel(session: PendingProtectionSession): string {
		const started = new Date(session.startedAt * 1000);
		if (Number.isNaN(started.getTime())) return '';
		return new Intl.DateTimeFormat(undefined, {
			day: 'numeric',
			month: 'short',
			hour: '2-digit',
			minute: '2-digit',
		}).format(started);
	}

	function loadoutName(loadoutId: string): string {
		return protection?.loadouts.find((loadout) => loadout.id === loadoutId)?.name ?? 'the chosen setup';
	}

	function hitsLabel(session: PendingProtectionSession): string {
		return `${session.defenceEventCount.toLocaleString()} ${session.defenceEventCount === 1 ? 'hit' : 'hits'}`;
	}
</script>

<div class="flex min-w-[390px] flex-col gap-3 text-white">
	<div class="border-b border-white/10 pb-2.5">
		<p class="text-xs font-semibold">Armour used</p>
		<p class="mt-1 text-[11px] text-white/45">
			Choose the setup worn. Its cost stays at whole-session level.
		</p>
	</div>

	{#if pending === null}
		<p class="py-2 text-[11px] text-white/40">Reading sessions...</p>
	{:else}
		<div class="flex items-center justify-between gap-3">
			<div class="min-w-0">
				<p class="truncate text-[11px]">This session</p>
				<p class="mt-0.5 text-[10px] text-white/35">
					{#if carriedLoadoutId}
						Recording under {loadoutName(carriedLoadoutId)}
					{:else if thisSessionPending}
						{hitsLabel(thisSessionPending)}
					{/if}
				</p>
			</div>
			<select
				class="min-w-52 shrink-0 rounded-[4px] border border-white/15 bg-white/5 px-2.5 py-1.5 text-xs text-white outline-none focus:border-accent/60"
				aria-label="Armour setup for this session"
				value={carriedLoadoutId ?? ''}
				disabled={assigningId !== null}
				onchange={(event) => {
					const select = event.currentTarget;
					if (select.value) void choose(select, sessionId, select.value);
				}}
			>
				<option value="">Choose armour setup</option>
				{#each protection?.loadouts ?? [] as loadout (loadout.id)}
					<option value={loadout.id}>{loadout.name}</option>
				{/each}
			</select>
		</div>

		{#each others as session (session.sessionId)}
			<div class="flex items-center justify-between gap-3">
				<div class="min-w-0">
					<p class="truncate text-[11px]">{session.name ?? 'Unnamed session'}</p>
					<p class="mt-0.5 text-[10px] text-white/35">
						{whenLabel(session)} · {hitsLabel(session)}
					</p>
				</div>
				<select
					class="min-w-52 shrink-0 rounded-[4px] border border-white/15 bg-white/5 px-2.5 py-1.5 text-xs text-white outline-none focus:border-accent/60"
					aria-label={`Armour setup for ${session.name ?? 'unnamed session'}`}
					disabled={assigningId !== null}
					onchange={(event) => {
						const select = event.currentTarget;
						if (select.value) void choose(select, session.sessionId, select.value);
					}}
				>
					<option value="">Choose armour setup</option>
					{#each protection?.loadouts ?? [] as loadout (loadout.id)}
						<option value={loadout.id}>{loadout.name}</option>
					{/each}
				</select>
			</div>
		{/each}

		{#if others.length > 0}
			<p class="text-[10px] leading-tight text-white/35">
				Sessions left without a setup keep their hits unpriced. Naming the same setup lets one
				reading cover them all.
			</p>
		{/if}
	{/if}

	{#if error}<p class="border-l-2 border-amber-400/70 pl-2 text-[10px] text-amber-200/80">{error}</p>{/if}

	{#if ready}
		<div class="flex justify-end">
			<Button
				size="sm"
				onclick={() => carriedLoadoutId && onassigned(carriedLoadoutId)}
			>
				Continue
			</Button>
		</div>
	{/if}
</div>
