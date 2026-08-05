/**
 * The session-definitions family: lifecycle over the deliberate activity
 * families tracked sessions are instances of (name, opt-in ad-hoc
 * segment flag, ordered activity roster). Thin wrappers over the
 * generated typed commands; argument shaping (string ids onto numeric
 * commands) only. Selection of the definition the next session starts
 * under is the tracking family's verb (`selectDefinition` in
 * `./tracking`).
 */

import type { SessionDefinition, SessionDefinitionInput } from './commands.gen';
import * as commands from './commands.gen';

export type {
	SessionRosterEntry,
	SessionRosterEntryInput,
	SessionRosterEntryKind,
} from './commands.gen';
export type { SessionDefinition, SessionDefinitionInput };

/** The definitions on offer: active only, which is everything the
 * picker and the authoring surface deal in. */
export async function getSessionDefinitions(): Promise<SessionDefinition[]> {
	return commands.sessionDefinitionsList(false);
}

/** Every definition including the archived ones, whose instances are
 * still real recorded play. Only the review surface asks for these, and
 * it shows them apart: they cannot take new sessions. */
export async function getAllSessionDefinitions(): Promise<SessionDefinition[]> {
	return commands.sessionDefinitionsList(true);
}

export const createSessionDefinition = commands.sessionDefinitionCreate;

export async function updateSessionDefinition(
	id: string,
	data: SessionDefinitionInput,
): Promise<SessionDefinition> {
	return commands.sessionDefinitionUpdate(Number(id), data);
}

export async function archiveSessionDefinition(id: string): Promise<SessionDefinition> {
	return commands.sessionDefinitionArchive(Number(id));
}

export async function restoreSessionDefinition(id: string): Promise<SessionDefinition> {
	return commands.sessionDefinitionRestore(Number(id));
}
