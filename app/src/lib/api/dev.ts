/**
 * Developer tools (hidden, developer-mode-gated). Each command is gated
 * on developer mode in the facade; when it is off the command rejects
 * with the `notFound` kind, so the metrics page treats that kind as
 * "gate closed".
 */

import * as commands from './commands.gen';

export type {
	AuctionFeeOverlayStatus,
	AuctionFeeResearchStatus,
	HistogramSnapshot,
	MetricsSnapshot,
} from './commands.gen';

export const getDevMetrics = commands.devMetrics;
export const startAuctionFeeResearch = commands.devAuctionFeeResearchStart;
export const stopAuctionFeeResearch = commands.devAuctionFeeResearchStop;
export const getAuctionFeeResearchStatus = commands.devAuctionFeeResearchStatus;
export const captureAuctionFeeResearchSample = commands.devAuctionFeeResearchCapture;
export const getAuctionFeeResearchOverlayStatus = commands.devAuctionFeeResearchOverlayStatus;

export async function getCrashReporting(): Promise<boolean> {
	return (await commands.devCrashReporting()).crash_reporting_enabled;
}

export async function setCrashReporting(enabled: boolean): Promise<boolean> {
	return (await commands.devSetCrashReporting(enabled)).crash_reporting_enabled;
}
