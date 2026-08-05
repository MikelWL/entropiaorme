/**
 * The review surface's state: which definition is under review, the
 * definitions it can switch between, and the morph lifecycle. The
 * instance list itself is the instances model, composed here with this
 * surface's definition as its scope.
 *
 * Review is management, not analysis. Its purpose is the one the record
 * is actually consulted for: a figure from the session just played looks
 * wrong, so the instance is opened, inspected, and then corrected or
 * deleted. Comparison, ranking, and per-instance economics belong to the
 * analytics surfaces and deliberately do not appear here.
 *
 * The definitions offered include the soft-deleted ones. Their instances
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

	/** The definition under review, which may be a soft-deleted one. */
	const definition = $derived(definitions.find((entry) => entry.id === definitionId) ?? null);

	/** The definitions that can still take a re-filed instance:
	 * everything on offer except the one being reviewed. */
	const moveTargets = $derived(
		definitions.filter((entry) => entry.isActive && entry.id !== definitionId),
	);

	/** Retired definitions are listed apart, after the offered ones, so
	 * the switcher never implies they can be played again. */
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

	/** Open the surface on a definition, which is the one the dashboard is
	 * already sitting on. Its instances load with it: the whole point of
	 * arriving here is that a session just played looked wrong, so what
	 * was recorded under it should already be on screen.
	 *
	 * A null id means the dashboard had no selection yet (the snapshot
	 * has not landed). Nothing is read then: an unscoped read would put
	 * the entire recorded history under a heading, a pager and an empty
	 * state that all claim to describe one definition. The surface asks
	 * for a choice instead, and its own switcher is the answer. */
	async function openReview(initialDefinitionId: string | null) {
		definitionId = initialDefinitionId;
		open = true;
		const reads = [loadDefinitions()];
		if (initialDefinitionId !== null) reads.push(instances.loadSessions());
		await Promise.all(reads);
	}

	/** Switch the definition under review, reloading its instances. */
	async function reviewDefinition(nextId: string) {
		if (nextId === definitionId) return;
		definitionId = nextId;
		await instances.loadSessions();
	}

	function close() {
		open = false;
		instances.collapseAll();
		instances.confirmDeleteId = null;
	}

	/** Re-file an instance, then refresh the definition list so both
	 * definitions' instance counts read true. */
	async function reassign(sessionId: string, targetId: string) {
		const moved = await instances.reassign(sessionId, targetId);
		if (moved) await loadDefinitions();
		return moved;
	}

	/** Deleting an instance changes its definition's count too. */
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
