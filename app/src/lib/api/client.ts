/**
 * The thrown error class of the backend client seam. Every typed command
 * rejects with a serialised `ApiErrorPayload` (`kind` + `message`); the
 * transport (`./invoke`) maps that payload onto this class, carrying the
 * kind and the message verbatim.
 */

import type { ApiErrorKind } from './commands.gen';

/**
 * The `kind` a thrown error carries: one of the generated contract kinds,
 * or `unknown` for a rejection outside the typed contract (an argument
 * that failed deserialisation, a missing command).
 */
export type ThrownErrorKind = ApiErrorKind | 'unknown';

export class ApiError extends Error {
	constructor(
		public kind: ThrownErrorKind,
		message: string,
	) {
		super(message);
		this.name = 'ApiError';
	}
}
