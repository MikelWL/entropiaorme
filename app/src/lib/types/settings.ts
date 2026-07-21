/**
 * Settings-surface types. The wire shapes re-export from the generated
 * bindings (the authoritative contract); only genuinely frontend-owned
 * names live here. `TrifectaPreset` and `MobTrackingMode` are the
 * historical consumer-facing names of the generated `TrifectaPresetView`
 * and `MobEntryMode`.
 */

export type {
	AppSettings,
	GameConnection,
	HarvestGuardrailInput,
	HarvestGuardrailSettings,
	MobEntryMode as MobTrackingMode,
	TrifectaPresetView as TrifectaPreset,
	TrifectaSettings,
} from '$lib/api/commands.gen';

/** Hotbar slot mapping: key "1"-"9" (and "0", stored last) to an
 * equipment-library id or null. The wire carries the stored JSON map
 * verbatim; `hotbarFromSettings` ($lib/api) narrows it to this shape. */
export type Hotbar = Record<string, number | null>;
