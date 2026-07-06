/**
 * The typed-command transport: the thin runtime under the generated
 * bindings (`commands.gen.ts`).
 *
 * Every typed command resolves with its declared payload or rejects
 * with the backend's serialised `ApiErrorPayload` (`kind` + `message`).
 * This wrapper maps that payload onto the thrown `ApiError` contract,
 * carrying the kind and the message verbatim.
 */

import { invoke } from '@tauri-apps/api/core';
import { ApiError } from './client';

/** The display message for the kinds that deliberately carry none. */
const MESSAGE_FOR_KIND: Record<string, string> = {
	internal: 'Internal Server Error',
	unavailable: 'backend substrate not ready',
};

export async function invokeCommand<T>(command: string, args: Record<string, unknown>): Promise<T> {
	try {
		return await invoke<T>(command, args);
	} catch (raw) {
		if (raw && typeof raw === 'object' && 'kind' in raw) {
			const payload = raw as { kind: string; message?: string };
			throw new ApiError(
				payload.kind,
				payload.message ?? MESSAGE_FOR_KIND[payload.kind] ?? payload.kind,
			);
		}
		// A rejection outside the typed contract (an argument that failed
		// deserialisation, a missing command): surface it verbatim.
		throw new ApiError('unknown', String(raw));
	}
}
