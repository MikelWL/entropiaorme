/**
 * Test-only reactive container: plain `.test.ts` suites are not compiled with
 * runes, so this gives them a runes-backed value to drive a view model's
 * reactive source (e.g. a table model's rows getter) from outside.
 */
export function reactiveBox<T>(initial: T): { value: T } {
	let value = $state(initial);
	return {
		get value() {
			return value;
		},
		set value(next: T) {
			value = next;
		},
	};
}
