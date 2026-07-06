/**
 * The two hand-written pieces of the backend client seam that outlive the
 * typed-command transport: the `ApiError` thrown across the whole facade
 * (mapped from each command's serialised error payload in `./invoke`), and
 * the manual-scan capture-preview helper.
 */

import { invoke } from '@tauri-apps/api/core';

/**
 * The typed error kinds the backend serialises across the command
 * boundary (`badRequest` | `notFound` | `conflict` | `internal` |
 * `unavailable`), plus `unknown` for a rejection outside the typed
 * contract.
 */
export class ApiError extends Error {
	constructor(
		public kind: string,
		message: string,
	) {
		super(message);
		this.name = 'ApiError';
	}
}

/** The manual-scan capture preview PNG for a page, as a base64 `data:` URL for
 * an `<img>` `src`. The capture returns raw image bytes rather than JSON, so it
 * rides its own `capture_png` command rather than the typed JSON command
 * surface. */
export async function manualSkillScanCapturePng(page: number): Promise<string> {
	const encoded = await invoke<string>('capture_png', { page });
	return `data:image/png;base64,${encoded}`;
}
