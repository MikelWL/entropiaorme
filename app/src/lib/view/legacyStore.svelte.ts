/**
 * Reactive bridge onto the frozen legacy store surface. New shared state is
 * authored with runes (ADR-0022) and must not import `svelte/store`, but a
 * handful of legacy modules still expose store-shaped state; consumers bridge
 * onto them structurally (the store contract is just `subscribe`) until the
 * legacy surface itself migrates.
 */

import { createSubscriber } from 'svelte/reactivity';

/** The structural store contract: `subscribe` with an immediate first call. */
export interface LegacyStore<T> {
	subscribe(run: (value: T) => void): () => void;
}

/**
 * The store's current value via a transient subscription (the store calls a
 * new subscriber synchronously). Non-reactive; the store-shaped equivalent of
 * reading a plain variable.
 */
export function readLegacyStore<T>(store: LegacyStore<T>): T {
	let value!: T;
	store.subscribe((v) => {
		value = v;
	})();
	return value;
}

/**
 * A reactive `{ current }` view of a legacy store. Subscribed while observed
 * from a reactive context (effects re-run on store writes) and detached when
 * unobserved; reads outside any reactive context fall back to a transient
 * fresh read, so `current` is never stale.
 */
export function legacyStoreState<T>(store: LegacyStore<T>): { readonly current: T } {
	let value: T | undefined;
	let live = false;
	const subscribe = createSubscriber((update) => {
		live = true;
		const unsubscribe = store.subscribe((v) => {
			value = v;
			update();
		});
		return () => {
			live = false;
			unsubscribe();
		};
	});
	return {
		get current(): T {
			subscribe();
			return live ? (value as T) : readLegacyStore(store);
		},
	};
}
