/** Protection setup, selection, and limited-layer observation commands. */

import * as commands from './commands.gen';

export type {
	ProtectionEconomyKind,
	ProtectionLoadout,
	ProtectionLoadoutInput,
	ProtectionObservation,
	ProtectionObservationInput,
	ProtectionObservationOutcome,
	ProtectionObservationSource,
	ProtectionOverview,
	ProtectionReconciliation,
	ProtectionReconciliationStatus,
	ProtectionScanResult,
	ProtectionSet,
	ProtectionSetInput,
	ProtectionSetKind,
	ProtectionSetRef,
} from './commands.gen';

export const getProtectionOverview = commands.protectionOverview;
export const createProtectionSet = commands.protectionSetCreate;
export const createProtectionLoadout = commands.protectionLoadoutCreate;
export const archiveProtectionSet = (id: string) => commands.protectionSetArchive(Number(id));
export const archiveProtectionLoadout = (id: string) =>
	commands.protectionLoadoutArchive(Number(id));
export const selectProtectionLoadout = (id: string) => commands.protectionSelect(Number(id));
export const confirmProtectionObservation = commands.protectionObservationConfirm;
export const scanTradeTerminalValue = commands.protectionTradeTerminalScan;
