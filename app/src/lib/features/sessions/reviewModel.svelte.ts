/**
 * The review surface's state: which family is under review, the families
 * it can switch between, and the morph lifecycle. The instance list
 * itself is the instances model, composed here with this surface's
 * definition as its scope.
 *
 * Review is management, not analysis. Its purpose is the one the record
 * is actually consulted for: a figure from the session just played looks
 * wrong, so the instance is opened, inspected, and then corrected or
 * deleted. Comparison, ranking, and per-instance economics belong to the
 * analytics surfaces and deliberately do not appear here.
 *
 * The families offered include the soft-deleted ones. Their instances
 * are real recorded play, and a definition that stopped being offered
 * would otherwise take its whole history out of reach; they are shown
 * apart, and cannot receive a re-filed instance.
 */

import { getAllSessionDefinitions, type SessionDefinition } from '$lib/api';
import { describeError } from '$lib/view/errorState';
import { createInstancesModel, type InstancesModel } from './instancesModel.svelte';

export interface ReviewModelDeps {
	/** Every definition, soft-deleted ones included. */
	listAllDefinitions(): Promise<SessionDefinition[]>;
	/** The scoped instance list; injected so tests compose the surface
	 * without reaching the backend. */
	createInstances(definitionId: () => string | null): InstancesModel;
}

export function createReviewModel(deps: ReviewModelDeps) {
	let open = $state(false);
	let definitionId = $state<string | null>(null);
	let definitions = $state<SessionDefinition[]>([]);
	let loadingDefinitions = $state(false);
	let error = $state<string | null>(null);

	const instances = deps.createInstances(() => definitionId);

	/** The family under review, which may be a soft-deleted one. */
	const definition = $derived(definitions.find((entry) => entry.id === definitionId) ?? null);

	/** The families that can still take a re-filed instance: everything
	 * on offer except the one being reviewed. */
	const moveTargets = $derived(
		definitions.filter((entry) => entry.isActive && entry.id !== definitionId),
	);

	/** Retired families are listed apart, after the offered ones, so the
	 * switcher never implies they can be played again. */
	const activeDefinitions = $derived(definitions.filter((entry) => entry.isActive));
	const retiredDefinitions = $derived(
		definitions.filter((entry) => !entry.isActive && entry.instanceCount > 0),
	);

	async function loadDefinitions() {
		loadingDefinitions = true;
		try {
			definitions = await deps.listAllDefinitions();
			error = null;
		} catch (e) {
			error = describeError(e, 'Failed to load sessions');
		} finally {
			loadingDefinitions = false;
		}
	}

	/** Open the surface on a family, defaulting to the one the dashboard
	 * is already sitting on. The instance list loads with it: the whole
	 * point of arriving here is that a session just played looked wrong,
	 * so its family's instances are what should already be on screen. */
	async function openReview(initialDefinitionId: string | null) {
		definitionId = initialDefinitionId;
		open = true;
		await Promise.all([loadDefinitions(), instances.loadSessions()]);
	}

	/** Switch the family under review, reloading its instances. */
	async function reviewDefinition(nextId: string) {
		if (nextId === definitionId) return;
		definitionId = nextId;
		await instances.loadSessions();
	}

	function close() {
		open = false;
		instances.collapseAll();
		instances.confirmDeleteId = null;
		instances.reassignTargetId = null;
	}

	/** Re-file an instance, then refresh the definition list so both
	 * families' instance counts read true. */
	async function reassign(sessionId: string, targetId: string) {
		const moved = await instances.reassign(sessionId, targetId);
		if (moved) await loadDefinitions();
		return moved;
	}

	/** Deleting an instance changes its family's count too. */
	async function remove(sessionId: string) {
		await instances.handleDelete(sessionId);
		await loadDefinitions();
	}

	return {
		instances,

		get open() {
			return open;
		},
		get definitionId() {
			return definitionId;
		},
		get definition() {
			return definition;
		},
		get definitions() {
			return definitions;
		},
		get activeDefinitions() {
			return activeDefinitions;
		},
		get retiredDefinitions() {
			return retiredDefinitions;
		},
		get moveTargets() {
			return moveTargets;
		},
		get loadingDefinitions() {
			return loadingDefinitions;
		},
		get error() {
			return error;
		},
		set error(value: string | null) {
			error = value;
		},

		openReview,
		reviewDefinition,
		close,
		reassign,
		remove,
		loadDefinitions,
	};
}

export type ReviewModel = ReturnType<typeof createReviewModel>;

/** The model wired to the live API: the app's composition (the
 * deps-injected factory above is the testable seam). */
export function createLiveReviewModel(): ReviewModel {
	return createReviewModel({
		listAllDefinitions: getAllSessionDefinitions,
		createInstances: (definitionId) => createInstancesModel({ definitionId }),
	});
}
