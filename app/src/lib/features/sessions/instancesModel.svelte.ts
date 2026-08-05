/**
 * The recorded-instances view model: the session list load, row
 * expand/collapse with its detail fetch, deletion, re-filing, and the
 * client-side pager. Presentation lives in the review surface; it
 * composes over this state.
 *
 * Optionally scoped to one definition, which is how the review surface
 * reads a family: the scope narrows the server's count as well as its
 * rows, so the pager reports the family's own bounds. Unscoped, this is
 * the whole recorded history.
 *
 * Paging is two-layered by design (the ledger tab's shape): the server
 * side stays keyset (an opaque cursor grows the loaded window on demand
 * as the pager steps past it), while the client-side pager over the
 * loaded window is the shared table model; the server's count gives the
 * pager its true bounds.
 */

import { deleteSession, getSessionDetail, getTrackingSessions, reassignSession } from '$lib/api';
import type { SessionDetail, TrackingSession } from '$lib/types/tracking';
import { describeError } from '$lib/view/errorState';
import { createTableModel } from '$lib/view/tableModel.svelte';

export const PAGE_SIZE = 10;

export interface InstancesModelOptions {
	/** The definition whose instances to read; null (or omitted) reads
	 * the whole history. Read at fetch time, so a caller can switch the
	 * family under review and reload. */
	definitionId?: () => string | null;
}

export function createInstancesModel(options: InstancesModelOptions = {}) {
	const scope = () => options.definitionId?.() ?? undefined;
	let sessions = $state<TrackingSession[]>([]);
	// The whole-table session count from the server, so the pager reports
	// true bounds rather than the loaded window's size.
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	// Keyset pagination: the cursor for the next server page (null once
	// every session is loaded), and whether a "load more" fetch is in flight.
	let nextCursor = $state<string | null>(null);
	let loadingMore = $state(false);
	let expandedSessionId = $state<string | null>(null);
	let expandedDetail = $state<SessionDetail | null>(null);
	let loadingDetail = $state(false);
	let confirmDeleteId = $state<string | null>(null);
	let deleting = $state(false);
	// The row whose "move to another session" chooser is open, and the
	// in-flight guard for the write itself.
	let reassignTargetId = $state<string | null>(null);
	let reassigning = $state(false);

	// Pure pager over the loaded window: no search, category, or sort, so
	// the paged rows keep the backend's ordering unchanged.
	const table = createTableModel<TrackingSession>({
		rows: () => sessions,
		pageSize: PAGE_SIZE,
	});

	async function loadSessions() {
		loading = true;
		error = null;
		// A reload is a fresh read of a possibly different family, so the
		// pager and any open row go back to the top rather than pointing
		// into the previous scope's window.
		table.page = 0;
		expandedSessionId = null;
		expandedDetail = null;
		try {
			const page = await getTrackingSessions(undefined, undefined, scope());
			sessions = page.sessions;
			nextCursor = page.nextCursor;
			total = page.total;
		} catch (e) {
			error = describeError(e, 'Failed to load sessions');
		} finally {
			loading = false;
		}
	}

	// Fetch the next keyset page and append it, growing the client
	// paginator's range. Older sessions stay reachable without loading the
	// whole history up front.
	async function loadMoreSessions() {
		if (!nextCursor || loadingMore) return;
		error = null;
		loadingMore = true;
		try {
			const page = await getTrackingSessions(nextCursor, undefined, scope());
			sessions = [...sessions, ...page.sessions];
			nextCursor = page.nextCursor;
			total = page.total;
		} catch (e) {
			error = describeError(e, 'Failed to load more sessions');
		} finally {
			loadingMore = false;
		}
	}

	// Pager bounds from the server total: the client pages the loaded
	// window, and stepping past it fetches the next keyset page on demand.
	const totalPages = $derived(Math.max(1, Math.ceil(total / PAGE_SIZE)));

	async function nextPage() {
		const nextStart = (table.page + 1) * PAGE_SIZE;
		if (nextStart >= total) return;
		if (nextStart >= sessions.length && nextCursor) await loadMoreSessions();
		if (nextStart < sessions.length) table.page++;
	}

	function prevPage() {
		if (table.page > 0) table.page--;
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
		if (deleting) return;
		error = null;
		deleting = true;
		try {
			await deleteSession(id);
			sessions = sessions.filter((s) => s.id !== id);
			total = Math.max(0, total - 1);
			if (expandedSessionId === id) {
				expandedSessionId = null;
				expandedDetail = null;
			}
		} catch (e) {
			error = describeError(e, 'Failed to delete session');
		}
		deleting = false;
		confirmDeleteId = null;
	}

	/** Move an instance to another family. Under a scoped read the row
	 * leaves this list, so it is dropped locally rather than refetched;
	 * unscoped it stays, and only its stamped name may have moved, which
	 * the reopened detail carries. */
	async function reassign(id: string, definitionId: string): Promise<boolean> {
		if (reassigning) return false;
		error = null;
		reassigning = true;
		try {
			await reassignSession(id, definitionId);
			if (scope() !== undefined) {
				sessions = sessions.filter((s) => s.id !== id);
				total = Math.max(0, total - 1);
				if (expandedSessionId === id) {
					expandedSessionId = null;
					expandedDetail = null;
				}
			} else if (expandedSessionId === id) {
				expandedDetail = await getSessionDetail(id).catch(() => expandedDetail);
			}
			return true;
		} catch (e) {
			error = describeError(e, 'Failed to move the session');
			return false;
		} finally {
			reassigning = false;
			reassignTargetId = null;
		}
	}

	function collapseAll() {
		expandedSessionId = null;
		expandedDetail = null;
	}

	function expandAtIndex(idx: number) {
		const target = table.pageRows[idx];
		if (!target) return;
		void toggleSession(target.id);
	}

	return {
		table,

		get sessions() {
			return sessions;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		set error(value: string | null) {
			error = value;
		},
		get nextCursor() {
			return nextCursor;
		},
		get loadingMore() {
			return loadingMore;
		},
		get total() {
			return total;
		},
		get totalPages() {
			return totalPages;
		},
		get expandedSessionId() {
			return expandedSessionId;
		},
		get expandedDetail() {
			return expandedDetail;
		},
		/** Writable so the detail view's own refetch (which a mob rename
		 * forces, because the backend regroups the breakdown) lands back
		 * here rather than only inside that component. */
		set expandedDetail(value: SessionDetail | null) {
			expandedDetail = value;
		},
		get loadingDetail() {
			return loadingDetail;
		},
		get confirmDeleteId() {
			return confirmDeleteId;
		},
		set confirmDeleteId(value: string | null) {
			confirmDeleteId = value;
		},
		get deleting() {
			return deleting;
		},
		get reassignTargetId() {
			return reassignTargetId;
		},
		set reassignTargetId(value: string | null) {
			reassignTargetId = value;
		},
		get reassigning() {
			return reassigning;
		},

		loadSessions,
		loadMoreSessions,
		nextPage,
		prevPage,
		toggleSession,
		handleDelete,
		reassign,
		collapseAll,
		expandAtIndex,
	};
}

export type InstancesModel = ReturnType<typeof createInstancesModel>;
