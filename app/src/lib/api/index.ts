/**
 * Backend API surface: the typed wrappers the app calls the backend
 * through, split per domain family.
 *
 * All backend communication goes through this module. Each wrapper
 * delegates to a generated typed command (`commands.gen.ts`, emitted
 * from the Rust command DTOs), which types the arguments and return
 * value against the backend contract at compile time; the generated
 * types are the authoritative contract, and the wrappers add only
 * argument shaping and the guide-mode read swap (`./guide`). The
 * shell's bespoke window/byte commands live in `./shell`.
 */

export * from './analytics';
export * from './character';
export { ApiError, type ThrownErrorKind } from './client';
export * from './codex';
// The generated shapes consumers reach through this barrel; `ActivityData`
// is the established consumer-facing name of `AnalyticsActivity`.
export type {
	AnalyticsActivity as ActivityData,
	ApiErrorKind,
	ManualMobSuggestion,
	MarketBreakEven,
	MarketBreakEvenCell,
	MarketCommitResult,
	MarketHistoryPoint,
	MarketHorizon,
	MarketLooterLevel,
	MarketMobRankingRow,
	MarketOverviewRow,
	MarketPastePreview,
	MarketPastePreviewRow,
	MarketReading,
	MarketSkippedLine,
	MarketWeaponBreakEven,
	SessionQuestLinkSuggestion,
} from './commands.gen';
export * from './dev';
export * from './equipment';
export * from './maps';
export * from './market';
export * from './quests';
export * from './scan';
export * from './settings';
// The shell's updater commands stay out of this barrel: $lib/updater
// owns that flow (phases, progress, stores) and imports them directly.
export {
	hideScanOverlay,
	manualSkillScanCapturePng,
	planetMapImage,
	showScanOverlay,
	toggleCartographyOverlay,
	toggleOverlay,
} from './shell';
export * from './tracking';
