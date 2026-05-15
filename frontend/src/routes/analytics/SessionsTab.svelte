<script lang="ts">
	import { onMount } from 'svelte';
	import { getTrackingSessions, getSessionDetail, deleteSession } from '$lib/api';
	import type { TrackingSession, SessionDetail } from '$lib/types/tracking';
	import { formatPed, formatDuration, formatDate } from '$lib/utils/format';
	import Badge from '$lib/components/Badge.svelte';
	import Card from '$lib/components/Card.svelte';
	import Button from '$lib/components/Button.svelte';
	import SessionDetailView from '$lib/components/SessionDetail.svelte';
	import { registerDemoApi, unregisterDemoApi } from '$lib/guide/state.svelte';

	let sessions = $state<TrackingSession[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let expandedSessionId = $state<string | null>(null);
	let expandedDetail = $state<SessionDetail | null>(null);
	let loadingDetail = $state(false);
	let confirmDeleteId = $state<string | null>(null);
	let deleting = $state(false);

	$effect(() => {
		loadSessions();
	});

	async function loadSessions() {
		loading = true;
		error = null;
		try {
			sessions = await getTrackingSessions();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load sessions';
		} finally {
			loading = false;
		}
	}

	async function toggleSession(id: string) {
		if (expandedSessionId === id) {
			expandedSessionId = null;
			expandedDetail = null;
			return;
		}

		expandedSessionId = id;
		expandedDetail = null;
		loadingDetail = true;
		try {
			expandedDetail = await getSessionDetail(id);
		} catch {
			expandedDetail = null;
		} finally {
			loadingDetail = false;
		}
	}

	async function handleDelete(id: string) {
		deleting = true;
		try {
			await deleteSession(id);
			sessions = sessions.filter((s) => s.id !== id);
			if (expandedSessionId === id) {
				expandedSessionId = null;
				expandedDetail = null;
			}
		} catch { /* ignore */ }
		deleting = false;
		confirmDeleteId = null;
	}

	// Pagination
	let currentPage = $state(1);
	const itemsPerPage = 10;

	let totalPages = $derived(Math.max(1, Math.ceil(sessions.length / itemsPerPage)));
	let paginatedSessions = $derived(
		sessions.slice((currentPage - 1) * itemsPerPage, currentPage * itemsPerPage)
	);

	$effect(() => {
		if (currentPage > totalPages) {
			currentPage = Math.max(1, totalPages);
		}
	});

	// Guide-mode demoApi: lets the analytics surface's sessions card drive
	// row expand/collapse programmatically for the looped animation.
	onMount(() => {
		registerDemoApi('analytics-sessions', {
			collapseAllSessions: () => {
				expandedSessionId = null;
				expandedDetail = null;
			},
			expandSessionAtIndex: (idx: number) => {
				const target = paginatedSessions[idx];
				if (!target) return;
				void toggleSession(target.id);
			}
		});
		return () => unregisterDemoApi('analytics-sessions');
	});
</script>

{#if loading}
	<p class="text-sm text-text-secondary">Loading sessions...</p>
{:else if error}
	<p class="text-sm text-error">{error}</p>
{:else if sessions.length === 0}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center">
			No sessions yet. Start tracking from the Dashboard to begin.
		</p>
	</Card>
{:else}
	<div class="space-y-4" data-guide-anchor="analytics-sessions-area">
		<div class="overflow-x-auto rounded-md border border-border">
			<table class="w-full text-sm text-left border-collapse">
				<thead class="bg-surface-hover/50 text-text-secondary text-xs uppercase tracking-wider">
					<tr>
						<th class="px-4 py-3 font-medium border-b border-border">Start Time</th>
						<th class="px-4 py-3 font-medium border-b border-border">Duration</th>
						<th class="px-4 py-3 font-medium border-b border-border">Mobs</th>
						<th class="px-4 py-3 font-medium border-b border-border text-right">Net</th>
						<th class="px-4 py-3 font-medium border-b border-border text-right">Badges</th>
						<th class="px-4 py-3 font-medium border-b border-border text-right w-10"></th>
					</tr>
				</thead>
				<tbody class="bg-surface">
					{#each paginatedSessions as session, i}
						{@const isExpanded = expandedSessionId === session.id}
						<tr
							data-guide-anchor="sessions-row"
							data-session-index={i}
							class="hover:bg-surface-hover/50 transition-colors cursor-pointer {isExpanded ? 'bg-surface-hover' : ''}"
							onclick={() => toggleSession(session.id)}
						>
							<td class="px-4 py-3 border-b border-border/50 tabular-nums">
								{formatDate(session.startTime)}
							</td>
							<td class="px-4 py-3 border-b border-border/50 text-text-secondary">
								{formatDuration(session.duration)}
							</td>
							<td class="px-4 py-3 border-b border-border/50">
								<div class="max-w-[200px] truncate" title={session.primaryMobs.join(', ')}>
									{#if session.primaryMobs.length > 0}
										{session.primaryMobs.join(', ')}
									{:else}
										<span class="text-text-tertiary italic">None</span>
									{/if}
								</div>
							</td>
							<td class="px-4 py-3 border-b border-border/50 text-right tabular-nums font-semibold {session.net >= 0 ? 'text-positive' : 'text-negative'}">
								{session.net >= 0 ? '+' : ''}{formatPed(session.net)}
							</td>
							<td class="px-4 py-3 border-b border-border/50">
								<div class="flex items-center justify-end gap-1">
									{#if session.globals > 0}
										<Badge variant="warning">{session.globals}G</Badge>
									{/if}
									{#if session.hofs > 0}
										<Badge variant="accent">{session.hofs}H</Badge>
									{/if}
								</div>
							</td>
							<td class="px-4 py-3 border-b border-border/50 text-right" onclick={(e) => e.stopPropagation()}>
								<div class="flex items-center justify-end gap-2">
									{#if confirmDeleteId === session.id}
										<div class="flex items-center gap-1">
											<Button size="sm" variant="danger" onclick={() => handleDelete(session.id)}>
												{#snippet children()}Delete{/snippet}
											</Button>
											<Button size="sm" variant="ghost" onclick={() => (confirmDeleteId = null)}>
												{#snippet children()}Cancel{/snippet}
											</Button>
										</div>
									{:else}
										<button
											type="button"
											class="icon-button-row p-1"
											onclick={() => (confirmDeleteId = session.id)}
											aria-label="Delete session"
											title="Delete session"
										>
											<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
												<path fill-rule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 10.23 1.482l.149-.022.841 10.518A2.75 2.75 0 007.596 19h4.807a2.75 2.75 0 002.742-2.53l.841-10.519.149.023a.75.75 0 00.23-1.482A41.03 41.03 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.5-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.5-.06l-.3 7.5a.75.75 0 101.5.06l.3-7.5z" clip-rule="evenodd" />
											</svg>
										</button>
									{/if}
									<svg
										data-guide-anchor="sessions-row-chevron"
										data-session-index={i}
										class="h-4 w-4 text-text-tertiary transition-transform duration-200 {isExpanded ? 'rotate-180' : ''}"
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 20 20"
										fill="currentColor"
									>
										<path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
									</svg>
								</div>
							</td>
						</tr>
						{#if isExpanded}
							<tr>
								<td colspan="6" class="p-0 border-b border-border/50">
									<div class="bg-surface-hover/30 p-4">
										{#if loadingDetail}
											<p class="text-xs text-text-tertiary animate-pulse">Loading detail...</p>
										{:else if expandedDetail}
											<SessionDetailView detail={expandedDetail} />
										{:else}
											<p class="text-xs text-text-tertiary">No detail available.</p>
										{/if}
									</div>
								</td>
							</tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>

		{#if totalPages > 1}
			<div class="flex items-center justify-between px-2">
				<span class="text-xs text-text-tertiary tabular-nums">
					Showing {(currentPage - 1) * itemsPerPage + 1}–{Math.min(currentPage * itemsPerPage, sessions.length)} of {sessions.length}
				</span>
				<div class="flex items-center gap-2">
					<Button
						size="sm"
						variant="ghost"
						disabled={currentPage === 1}
						onclick={() => currentPage--}
					>
						Previous
					</Button>
					<span class="text-xs font-medium px-2">{currentPage} / {totalPages}</span>
					<Button
						size="sm"
						variant="ghost"
						disabled={currentPage === totalPages}
						onclick={() => currentPage++}
					>
						Next
					</Button>
				</div>
			</div>
		{/if}
	</div>
{/if}
