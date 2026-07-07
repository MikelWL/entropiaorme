/** Shared types used across multiple pages */

/** PED amounts are always numbers (e.g., 142.31 = 142.31 PED) */
export type Ped = number;

/** PEC amounts (1/100 PED) */
export type Pec = number;

/** PES (Project Entropia Skill) amounts: non-liquid skill-progress
 * denomination, distinct from PED. Stays out of liquid P&L by design. */
export type Pes = number;

/** ISO 8601 date string */
export type ISODate = string;

/** Duration in seconds */
export type Seconds = number;

/** Percentage as a decimal (0.95 = 95%) */
export type Ratio = number;

/** Cooldown state for quests (a frontend derivation, not a wire shape) */
export type CooldownStatus = 'ready' | 'cooling' | 'no_cooldown';

// The wire vocabularies re-export from the generated bindings (the
// authoritative contract).
export type { NotableEventCategory, NotableEventType, Trend } from '$lib/api/commands.gen';
