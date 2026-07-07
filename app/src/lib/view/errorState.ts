/**
 * The one place a caught `unknown` becomes user-facing text. Route loads and
 * mutations funnel their failures through this before handing the string to
 * `ErrorNotice`, so every surface degrades with the same behaviour: an
 * `Error` (including `ApiError`) contributes its message, a plain string
 * stands as-is, and anything else falls back to a generic line rather than
 * leaking a stringified object.
 */
export function describeError(error: unknown, fallback = 'Something went wrong'): string {
	if (error instanceof Error && error.message.trim() !== '') {
		return error.message;
	}
	if (typeof error === 'string' && error.trim() !== '') {
		return error;
	}
	return fallback;
}
