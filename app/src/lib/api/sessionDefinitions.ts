/**
 * The session-definitions family: CRUD over the deliberate activity
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

export const getSessionDefinitions = commands.sessionDefinitionsList;
export const createSessionDefinition = commands.sessionDefinitionCreate;

export async function updateSessionDefinition(
	id: string,
	data: SessionDefinitionInput,
): Promise<SessionDefinition> {
	return commands.sessionDefinitionUpdate(Number(id), data);
}

export async function deleteSessionDefinition(id: string): Promise<void> {
	await commands.sessionDefinitionDelete(Number(id));
}
