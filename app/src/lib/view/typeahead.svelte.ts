export interface TypeaheadOptions<T> {
	search: (query: string) => Promise<T[]>;
	/** Debounce between the last keystroke and the search firing. Default 200ms. */
	debounceMs?: number;
	/** Minimum trimmed query length before a search fires. Default 2. */
	minLength?: number;
	/** Display label for an item; `select` snaps the query onto it when provided. */
	labelOf?: (item: T) => string;
}

export interface Typeahead<T> {
	query: string;
	readonly results: T[];
	readonly selected: T | null;
	readonly loading: boolean;
	readonly error: string | null;
	select(item: T): void;
	clear(): void;
	destroy(): void;
}

/**
 * Debounced async picker view model. No search fires while an item is selected
 * or the trimmed query is under `minLength`; responses are dropped as stale
 * unless the query they answered still matches (and no selection or teardown
 * intervened), so out-of-order resolutions can never overwrite fresher state.
 */
export function createTypeahead<T>(options: TypeaheadOptions<T>): Typeahead<T> {
	const { search, debounceMs = 200, minLength = 2, labelOf } = options;

	let query = $state('');
	let results = $state<T[]>([]);
	let selected = $state<T | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	let timer: ReturnType<typeof setTimeout> | null = null;
	let destroyed = false;

	function cancelPending(): void {
		if (timer !== null) {
			clearTimeout(timer);
			timer = null;
		}
	}

	function isCurrent(searched: string): boolean {
		return !destroyed && selected === null && query.trim() === searched;
	}

	async function run(searched: string): Promise<void> {
		try {
			const items = await search(searched);
			if (isCurrent(searched)) {
				results = items;
				error = null;
			}
		} catch (e) {
			if (isCurrent(searched)) {
				results = [];
				error = e instanceof Error ? e.message : String(e);
			}
		} finally {
			if (isCurrent(searched)) loading = false;
		}
	}

	function schedule(): void {
		cancelPending();
		const trimmed = query.trim();
		if (selected !== null || trimmed.length < minLength) {
			results = [];
			loading = false;
			error = null;
			return;
		}
		timer = setTimeout(() => {
			timer = null;
			loading = true;
			void run(trimmed);
		}, debounceMs);
	}

	return {
		get query() {
			return query;
		},
		set query(value: string) {
			if (value === query) return;
			query = value;
			schedule();
		},
		get results() {
			return results;
		},
		get selected() {
			return selected;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		select(item: T) {
			cancelPending();
			selected = item;
			if (labelOf) query = labelOf(item);
			results = [];
			loading = false;
			error = null;
		},
		clear() {
			cancelPending();
			selected = null;
			query = '';
			results = [];
			loading = false;
			error = null;
		},
		destroy() {
			destroyed = true;
			cancelPending();
		},
	};
}
