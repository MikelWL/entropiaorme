/**
 * The session-facet view model: the independent, co-recorded attributions
 * a tracking session carries (session type + designated name, skill
 * boost, player-drawn segment, quest focus) and the writes that move
 * them. The quest facet is user-declared: the picker focuses and
 * unfocuses in-progress quests (completion closes a focused stretch by
 * itself, backend-side).
 *
 * The facets are independent by construction, so each control writes only
 * its own value and carries the others through unchanged. Each facet may
 * be edited live only where its stamp grain is finer than the session:
 * the boost stamps each skill gain and a segment is a slice of the
 * session, so both float; the session type (and the name it writes) is
 * session-grain, so the backend fixes it once a session runs (409) and
 * correction is a post-hoc move. The model respects these rather than
 * duplicating them.
 *
 * The satellite-window plumbing (anchors, popup lifecycle) stays with the
 * overlay route; this model owns the state and the writes.
 */

import { ApiError } from '$lib/api';

export interface SessionFacetsDeps {
	/** The facets currently in force, as the snapshot reports them.
	 * `segment` is the open segment's name (null: none open; a segment
	 * exists only while its session runs); `definitionId` is the
	 * selected session type (stringified id). */
	readFacets: () => {
		name: string | null;
		definitionId: string | null;
		boost: number | null;
		segment: string | null;
	};
	/** Whether a session is running (gates the session-type lock). */
	isSessionActive: () => boolean;
	/** Re-read the snapshot after a successful write. */
	refresh: () => Promise<unknown>;
	/** Full-state facet write: a null clears its facet. */
	setSessionConfig: (name: string | null, boost: number | null) => Promise<unknown>;
	/** Select the session type the next session starts under (null
	 * withdraws); the backend writes the name facet with it. */
	selectDefinition: (id: number | null) => Promise<unknown>;
	/** Open a segment on the running session, closing any standing one; a
	 * null name is auto-numbered ("Segment N") by the backend, and the
	 * acknowledgement echoes the name now in force so the control can
	 * render it without waiting on a snapshot round-trip. */
	openSegment: (name: string | null) => Promise<{ segmentName: string | null }>;
	/** Close the open segment. */
	closeSegment: () => Promise<unknown>;
	/** Rename the open segment live. */
	renameSegment: (name: string) => Promise<unknown>;
	/** Focus a quest (exclusive switch, or additive join). */
	focusQuest: (questId: number, additive: boolean) => Promise<unknown>;
	/** End one quest's focus, leaving siblings running. */
	unfocusQuest: (questId: number) => Promise<unknown>;
}

function describe(error: unknown, fallback: string): string {
	return error instanceof ApiError || error instanceof Error ? error.message : fallback;
}

export function createSessionFacets(deps: SessionFacetsDeps) {
	// The session-type selection's in-flight guard (the chip disables
	// while a selection write lands).
	let savingDefinition = $state(false);

	// The boost's edit buffer. The boost edits live: a pill expiring is a
	// genuine change worth recording, and the session keeps the latest
	// declaration.
	let boostDraft = $state('');
	let savingBoost = $state(false);

	// The segment's edit buffer. One field serves both moments: with no
	// segment open it holds the prospective next name (blank means "let
	// the backend auto-number"), and with one open it renames it live,
	// which the facet-grain rule allows because a segment is finer than
	// the session.
	let segmentDraft = $state('');
	let savingSegment = $state(false);

	// The quest-focus writes' in-flight guard (the picker disables its
	// trigger while one lands).
	let savingFocus = $state(false);

	// One channel for every facet write failure, so a refusal is surfaced
	// beside the controls rather than swallowed.
	let facetError = $state<string | null>(null);

	/** The boost currently in force, as a write wants it. Reading the
	 * snapshot (not the draft) keeps a name write from moving the boost as
	 * a side effect. Three-state: null withdraws the declaration, 0
	 * declares deliberately-unboosted play, a positive number declares a
	 * magnitude. */
	function currentBoost(): number | null {
		const value = deps.readFacets().boost;
		return value !== null && value !== undefined && value >= 0 ? value : null;
	}

	async function write(name: string | null, boost: number | null) {
		await deps.setSessionConfig(name, boost);
		await deps.refresh();
	}

	/** Select the session type for the next session (null withdraws both
	 * the selection and the name it wrote). The route's picker menu
	 * passes toggle semantics down to this single write. */
	async function selectDefinition(id: string | null) {
		savingDefinition = true;
		facetError = null;
		try {
			await deps.selectDefinition(id === null ? null : Number(id));
			await deps.refresh();
		} catch (error) {
			facetError = describe(
				error,
				id === null ? 'Failed to clear session type' : 'Failed to select session type',
			);
		}
		savingDefinition = false;
	}

	async function commitBoost() {
		// The empty field and a typed 0 are DIFFERENT declarations: empty
		// withdraws (claims nothing), 0 declares deliberately-unboosted
		// play, which is the baseline a boost's effect is measured
		// against. Anything unparseable or negative falls back to a
		// withdrawal rather than inventing a magnitude.
		const trimmed = boostDraft.trim();
		const parsed = trimmed ? Number.parseInt(trimmed, 10) : Number.NaN;
		const next = Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
		if (next === currentBoost()) {
			// Normalise the buffer even when nothing moved, so a stray
			// "abc" or " 50 " does not linger as if it were persisted.
			boostDraft = next === null ? '' : String(next);
			return;
		}
		savingBoost = true;
		facetError = null;
		try {
			await write(deps.readFacets().name, next);
		} catch (error) {
			facetError = describe(error, 'Failed to set skill boost');
		}
		savingBoost = false;
		boostDraft = next === null ? '' : String(next);
	}

	/** The open segment's name as the snapshot reports it (null: none). */
	function currentSegment(): string | null {
		return deps.readFacets().segment ?? null;
	}

	/** The field's commit (Enter): with a segment open it renames; with
	 * none it opens one, a blank draft leaving the backend to
	 * auto-number.
	 *
	 * The buffer is set from the write's own result, never left to the
	 * snapshot-driven sync: that sync fires while `savingSegment` still
	 * guards it (the refresh happens inside this call), so relying on it
	 * would leave the field stale until the next unrelated snapshot
	 * frame. The open acknowledgement echoes the applied name for
	 * exactly this. */
	async function commitSegment() {
		const name = segmentDraft.trim();
		const open = currentSegment();
		savingSegment = true;
		facetError = null;
		try {
			if (open !== null) {
				// A blank or unchanged rename is a normalisation, not a
				// write: an open segment always carries a name.
				if (!name || name === open) {
					segmentDraft = open;
					return;
				}
				await deps.renameSegment(name);
				await deps.refresh();
				segmentDraft = name;
			} else {
				const applied = await deps.openSegment(name || null);
				await deps.refresh();
				segmentDraft = applied.segmentName ?? '';
			}
		} catch (error) {
			facetError = describe(
				error,
				open !== null ? 'Failed to rename segment' : 'Failed to start segment',
			);
		} finally {
			savingSegment = false;
		}
	}

	/** The one-click boundary: start the next segment, closing any
	 * standing one (segments are sequential). With a segment open the
	 * new one is always auto-numbered; with none open a typed draft is
	 * honoured as its name. The buffer takes the echoed name directly
	 * (see commitSegment for why the sync cannot be relied on here). */
	async function nextSegment() {
		const name = currentSegment() === null ? segmentDraft.trim() : '';
		savingSegment = true;
		facetError = null;
		try {
			const applied = await deps.openSegment(name || null);
			await deps.refresh();
			segmentDraft = applied.segmentName ?? '';
		} catch (error) {
			facetError = describe(error, 'Failed to start segment');
		} finally {
			savingSegment = false;
		}
	}

	async function closeSegment() {
		savingSegment = true;
		facetError = null;
		try {
			await deps.closeSegment();
			await deps.refresh();
			segmentDraft = '';
		} catch (error) {
			facetError = describe(error, 'Failed to close segment');
		} finally {
			savingSegment = false;
		}
	}

	/** Focus a quest: the one-tap switch (exclusive over quests), or an
	 * additive join when the play ahead advances two quests at once. */
	async function focusQuest(questId: number, additive: boolean) {
		savingFocus = true;
		facetError = null;
		try {
			await deps.focusQuest(questId, additive);
			await deps.refresh();
		} catch (error) {
			facetError = describe(error, 'Failed to focus quest');
		} finally {
			savingFocus = false;
		}
	}

	/** End one quest's focus, leaving siblings running. */
	async function unfocusQuest(questId: number) {
		savingFocus = true;
		facetError = null;
		try {
			await deps.unfocusQuest(questId);
			await deps.refresh();
		} catch (error) {
			facetError = describe(error, 'Failed to unfocus quest');
		} finally {
			savingFocus = false;
		}
	}

	/** Start a segment from a recalled preset name (closes any standing
	 * one, like every segment open). The buffer takes the echoed name
	 * directly, as in commitSegment. */
	async function applySegmentPreset(label: string) {
		savingSegment = true;
		facetError = null;
		try {
			const applied = await deps.openSegment(label);
			await deps.refresh();
			segmentDraft = applied.segmentName ?? '';
		} catch (error) {
			facetError = describe(error, 'Failed to start segment');
		} finally {
			savingSegment = false;
		}
	}

	return {
		get savingDefinition() {
			return savingDefinition;
		},
		get boostDraft() {
			return boostDraft;
		},
		set boostDraft(value: string) {
			boostDraft = value;
		},
		get savingBoost() {
			return savingBoost;
		},
		/** Whether the session type (and the name it writes) may still be
		 * set. Both are session-grain, so a live edit could only rewrite
		 * the whole session's history: they are fixed once a session runs,
		 * and correction is a post-hoc move on the session record. The
		 * boost has no such flag because its grain is finer than the
		 * session, so it always edits. */
		get definitionEditable() {
			return !deps.isSessionActive();
		},
		get segmentDraft() {
			return segmentDraft;
		},
		set segmentDraft(value: string) {
			segmentDraft = value;
		},
		get savingSegment() {
			return savingSegment;
		},
		get savingFocus() {
			return savingFocus;
		},
		get facetError() {
			return facetError;
		},
		set facetError(value: string | null) {
			facetError = value;
		},

		/** Keep the boost buffer in step with the persisted value while the
		 * user is not editing it. A persisted 0 renders as "0", not as the
		 * empty field: it is a declaration, not the absence of one. */
		syncBoostDraft() {
			if (savingBoost) return;
			const persisted = deps.readFacets().boost;
			boostDraft =
				persisted !== null && persisted !== undefined && persisted >= 0 ? String(persisted) : '';
		},

		/** Keep the segment buffer in step with the open segment's name
		 * while the user is not editing it (a next-segment click
		 * renumbered it; a close or session stop emptied it). Runs only
		 * when the persisted name moves, so a prospective name being
		 * typed while no segment is open survives snapshot refreshes. */
		syncSegmentDraft() {
			if (savingSegment) return;
			segmentDraft = currentSegment() ?? '';
		},

		/** Blur commits only a rename: with no segment open, clicking
		 * away must not start one (opening is a deliberate Enter or
		 * next-segment click), and the typed prospective name is kept. */
		async handleSegmentBlur() {
			if (currentSegment() !== null) await commitSegment();
		},

		selectDefinition,
		commitBoost,
		commitSegment,
		nextSegment,
		closeSegment,
		focusQuest,
		unfocusQuest,
		applySegmentPreset,
	};
}
