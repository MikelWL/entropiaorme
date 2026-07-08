import { describe, expect, it } from 'vitest';
import { legacyStoreState, readLegacyStore } from './legacyStore.svelte';

/** A minimal store-contract implementation (subscribe with immediate call). */
function miniStore<T>(initial: T) {
	let value = initial;
	const subscribers = new Set<(v: T) => void>();
	return {
		subscribe(run: (v: T) => void) {
			subscribers.add(run);
			run(value);
			return () => {
				subscribers.delete(run);
			};
		},
		set(v: T) {
			value = v;
			for (const run of subscribers) run(v);
		},
		get subscriberCount() {
			return subscribers.size;
		},
	};
}

describe('readLegacyStore', () => {
	it('returns the current value and leaves no subscription behind', () => {
		const store = miniStore(3);
		expect(readLegacyStore(store)).toBe(3);
		expect(store.subscriberCount).toBe(0);
		store.set(7);
		expect(readLegacyStore(store)).toBe(7);
		expect(store.subscriberCount).toBe(0);
	});
});

describe('legacyStoreState', () => {
	it('reads fresh outside a reactive context', () => {
		const store = miniStore('a');
		const state = legacyStoreState(store);
		expect(state.current).toBe('a');
		store.set('b');
		expect(state.current).toBe('b');
	});

	it('holds no standing subscription when only read imperatively', () => {
		const store = miniStore(1);
		const state = legacyStoreState(store);
		expect(state.current).toBe(1);
		expect(store.subscriberCount).toBe(0);
	});
});
