/** Protection setup, selection, and limited-layer observation commands. */

import type { ProtectionLoadoutInput, ProtectionSetInput } from './commands.gen';
import * as commands from './commands.gen';

export type {
	ProtectionCostAllocation,
	ProtectionCostWindow,
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
	ProtectionRepairInput,
	ProtectionRepairOutcome,
	ProtectionScanResult,
	ProtectionSet,
	ProtectionSetInput,
	ProtectionSetKind,
	ProtectionSetRef,
} from './commands.gen';

export const getProtectionOverview = commands.protectionOverview;
export const createProtectionSet = commands.protectionSetCreate;
export const updateProtectionSet = (id: string, input: ProtectionSetInput) =>
	commands.protectionSetUpdate(Number(id), input);
export const createProtectionLoadout = commands.protectionLoadoutCreate;
export const updateProtectionLoadout = (id: string, input: ProtectionLoadoutInput) =>
	commands.protectionLoadoutUpdate(Number(id), input);
export const archiveProtectionSet = (id: string) => commands.protectionSetArchive(Number(id));
export const archiveProtectionLoadout = (id: string) =>
	commands.protectionLoadoutArchive(Number(id));
export const selectProtectionLoadout = (id: string) => commands.protectionSelect(Number(id));
export const assignSessionProtectionLoadout = (sessionId: string, loadoutId: string) =>
	commands.protectionAssignSessionLoadout(sessionId, Number(loadoutId));
export const confirmProtectionObservation = commands.protectionObservationConfirm;
export const confirmProtectionRepair = commands.protectionRepairConfirm;
export const scanTradeTerminalValue = commands.protectionTradeTerminalScan;
