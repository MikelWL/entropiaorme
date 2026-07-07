/**
 * The manual scan flow (public, user-driven page-by-page capture).
 * Thin wrappers over the generated typed commands. A logical refusal
 * rides the returned status' `error` field (the scanner never throws
 * for one), so every caller reads `.error` first and the status fields
 * defensively.
 */

import type { ScanStatus } from './commands.gen';
import * as commands from './commands.gen';

export type { ScanPhase, SkillScanPending } from './commands.gen';

/** The manual-scan status shape, as the scan commands answer it. */
export type ScanManualStatus = ScanStatus;

export const getManualSkillScanStatus = commands.scanStatus;
export const cancelManualSkillScan = commands.scanCancel;
export const undoManualSkillCapture = commands.scanUndo;
export const processManualSkillScan = commands.scanProcess;
export const acceptManualSkillScan = commands.scanAccept;
export const rejectManualSkillScan = commands.scanReject;
export const getManualSkillScanPending = commands.scanPending;
export const captureManualSkillPage = commands.scanCapture;
export const setSpacebarCapture = commands.scanSpacebarCapture;

export async function startManualSkillScan(pageCount?: number): Promise<ScanStatus> {
	return commands.scanStart(pageCount ?? null);
}
