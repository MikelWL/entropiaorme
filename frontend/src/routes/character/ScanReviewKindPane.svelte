<script lang="ts">
	import {
		acceptManualSkillScan,
		rejectManualSkillScan,
		getManualSkillScanPending,
		getCharacterSkills,
	} from '$lib/api';
	import type { SkillLevel } from '$lib/types/analytics';
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';

	let { onComplete }: { onComplete: () => void } = $props();

	// Three-stop diff: anchor (last scan) → believed (running sum) → scanned (new).
	// The most useful diff for validating the chatlog tracker is `scanned - believed`
	// (the "correction") — it surfaces where the running sum was off vs. ground truth.
	type DiffRow = {
		name: string;
		anchor: number | null;
		believed: number | null;
		scanned: number;
		isAttribute: boolean;
	};

	let rows = $state<DiffRow[]>([]);
	let loading = $state(true);
	let busy = $state(false);
	let error = $state<string | null>(null);

	async function load() {
		loading = true;
		error = null;
		try {
			const [pending, current] = await Promise.all([
				getManualSkillScanPending(),
				getCharacterSkills(),
			]);
			if (!pending) {
				error = 'No pending skill scan result';
				rows = [];
				return;
			}
			const byName = new Map<string, SkillLevel>(current.map((s) => [s.name, s]));
			const built: DiffRow[] = Object.entries(pending.skills).map(([name, scanned]) => {
				const prev = byName.get(name);
				return {
					name,
					anchor: prev?.anchorLevel ?? null,
					believed: prev?.level ?? null,
					scanned,
					isAttribute: prev?.isAttribute ?? false,
				};
			});
			rows = sortByCorrection(built);
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
			rows = [];
		} finally {
			loading = false;
		}
	}

	// Default sort: largest absolute correction (scanned - believed) first — that's
	// where the running sum diverged most from ground truth, which is where the
	// user's eye should land first when validating.
	function sortByCorrection(list: DiffRow[]): DiffRow[] {
		return [...list].sort((a, b) => Math.abs(correction(b)) - Math.abs(correction(a)));
	}

	$effect(() => {
		load();
	});

	async function accept() {
		busy = true;
		error = null;
		try {
			const result = await acceptManualSkillScan();
			if (result.error) {
				error = result.error;
			} else {
				onComplete();
			}
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			busy = false;
		}
	}

	async function reject() {
		busy = true;
		error = null;
		try {
			const result = await rejectManualSkillScan();
			if (result.error) {
				error = result.error;
			} else {
				onComplete();
			}
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			busy = false;
		}
	}

	// `correction` is what gets persisted as the new anchor minus what we believed
	// the level was. Positive: tracker undercounted; negative: tracker overcounted.
	function correction(row: DiffRow): number {
		if (row.believed === null) return 0;
		return row.scanned - row.believed;
	}

	// `runningGain` is anchor → believed: how much the chatlog tracker accumulated
	// since the last scan. Useful as context next to the believed value.
	function runningGain(row: DiffRow): number | null {
		if (row.anchor === null || row.believed === null) return null;
		return row.believed - row.anchor;
	}

	function formatLevel(value: number | null): string {
		if (value === null) return '—';
		return value.toFixed(2);
	}

	function formatSignedDelta(value: number | null): string {
		if (value === null) return '—';
		if (Math.abs(value) < 0.005) return '0.00';
		return (value > 0 ? '+' : '') + value.toFixed(2);
	}

	function deltaColor(value: number | null): string {
		if (value === null || Math.abs(value) < 0.005) return 'text-text-tertiary';
		return value > 0 ? 'text-success' : 'text-warning';
	}
</script>

<div class="flex h-full flex-col">
	<div class="mb-2 flex items-baseline justify-between">
		<h3 class="text-sm font-semibold uppercase tracking-wide text-text-secondary">
			Skills review
		</h3>
		<span class="text-xs text-text-tertiary tabular-nums">{rows.length} rows</span>
	</div>

	{#if loading}
		<p class="py-8 text-center text-sm text-text-tertiary">Loading review…</p>
	{:else if error}
		<p class="py-8 text-center text-sm text-warning">{error}</p>
	{:else if rows.length === 0}
		<p class="py-8 text-center text-sm text-text-tertiary">No rows extracted.</p>
	{:else}
		<div class="flex-1 overflow-y-auto rounded border border-border">
			<table class="w-full text-sm">
				<thead class="sticky top-0 bg-surface">
					<tr class="border-b border-border">
						<th class="py-2 px-3 text-left text-xs font-medium uppercase tracking-wide text-text-secondary">Name</th>
						<th class="py-2 px-3 text-right text-xs font-medium uppercase tracking-wide text-text-secondary">Anchor</th>
						<th class="w-4"></th>
						<th class="py-2 px-3 text-right text-xs font-medium uppercase tracking-wide text-text-secondary">Believed</th>
						<th class="w-4"></th>
						<th class="py-2 px-3 text-right text-xs font-medium uppercase tracking-wide text-text-secondary">Scanned</th>
						<th class="py-2 px-3 text-right text-xs font-medium uppercase tracking-wide text-text-secondary" title="Scanned − Believed (running sum vs. ground truth)">Δ</th>
					</tr>
				</thead>
				<tbody>
					{#each rows as row (row.name)}
						{@const corr = correction(row)}
						{@const gain = runningGain(row)}
						{@const unchanged = row.anchor !== null && Math.abs(row.scanned - row.anchor) < 0.005}
						<tr class="border-b border-border/50 transition-colors hover:bg-surface-hover/40" class:opacity-50={unchanged}>
							<td class="py-2 px-3">
								<div class="flex items-center gap-2">
									<span class="text-text">{row.name}</span>
									{#if row.isAttribute}
										<Badge variant="neutral">Attribute</Badge>
									{/if}
									{#if row.anchor === null}
										<Badge variant="accent">New</Badge>
									{/if}
								</div>
							</td>
							<td class="py-2 px-3 text-right tabular-nums text-text-secondary">{formatLevel(row.anchor)}</td>
							<td class="text-center text-text-tertiary text-xs">→</td>
							<td class="py-2 px-3 text-right tabular-nums">
								<div class="flex flex-col items-end leading-tight">
									<span class="text-text">{formatLevel(row.believed)}</span>
									{#if gain !== null && Math.abs(gain) >= 0.005}
										<span class="text-[10px] {deltaColor(gain)}">{formatSignedDelta(gain)}</span>
									{/if}
								</div>
							</td>
							<td class="text-center text-text-tertiary text-xs">→</td>
							<td class="py-2 px-3 text-right tabular-nums font-medium text-accent">{formatLevel(row.scanned)}</td>
							<td class="py-2 px-3 text-right tabular-nums {deltaColor(corr)}">
								{row.believed === null ? '—' : formatSignedDelta(corr)}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	<div class="mt-3 flex items-center justify-end gap-2">
		<Button variant="ghost" onclick={reject} disabled={busy}>
			{#snippet children()}Reject{/snippet}
		</Button>
		<Button onclick={accept} disabled={busy || loading || rows.length === 0}>
			{#snippet children()}Accept{/snippet}
		</Button>
	</div>
</div>
