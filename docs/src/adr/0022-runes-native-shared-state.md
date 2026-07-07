# ADR-0022: Runes-native shared state

- Status: Accepted
- Context: the frontend's component-level reactivity is fully runes-native ([ADR-0006](0006-tauri-svelte-frontend.md)): compiler-enforced runes mode across the tree, `$state`/`$derived`/`$effect` throughout, zero legacy reactive statements. Shared state *between* components, however, predated that migration and split across two idioms: eight module-scoped `svelte/store` `writable()` modules (news, updater, theme, stats customisation, activity archive, the tracking and scan stores) beside one runes-class module (the guide state). No stated rule said which idiom new state should use.

## Context and problem statement

Svelte 5 makes stores optional: a `.svelte.ts` module holding `$state` fields is the platform's native shape for shared reactive state, with the same fine-grained per-property tracking components already use, no subscription lifecycle, and no `$`-prefix auto-subscription syntax to learn alongside runes. The store contract survives for interoperability, but a codebase carrying both idioms pays a standing tax: two vocabularies for one concept, two testing styles, and a fork that deepens with every new feature that has to pick a side. Review guidance that says "prefer runes where the surrounding code uses runes" entrenches the split rather than resolving it, because the surrounding code is exactly what is split.

## Decision

Shared frontend state is authored as **runes-native `.svelte.ts` modules** (a class or closure over `$state`/`$derived`, per the guide-state module's established shape). The `svelte/store` surface is **frozen**: the modules importing it at the time of this decision are enumerated in a whole-tree guard (`cargo xtask no-new-writable`, wired into the required CI lint job and the local pre-commit hooks), which fails on any importer outside that list and on any stale list entry. Migrating a legacy module removes its entry; nothing adds one. The reactive view-model and snapshot-store factories introduced alongside this decision are runes-native from birth, so decomposition work builds on the target idiom rather than adding to the legacy surface.

## Consequences

New state has one obvious shape, and the compiler owns its reactivity semantics end to end: consumers read properties directly instead of auto-subscribing, and derived values live where the state lives. The eight frozen modules keep working unchanged until each migrates; the guard makes the migration's direction structural rather than aspirational, and its stale-entry check means the allowlist cannot silently outlive the modules it froze. The store idiom's one genuine convenience, `$store` auto-subscription in templates, is given up in exchange for uniform property access; at the module sizes in question the migration cost is mechanical and the suite pins the semantics.
