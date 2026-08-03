-- Quest families: variants of one repeatable slot, cooling as a unit.
--
-- Some quest givers offer one slot per day per line ("Daily Hunting 1"),
-- handing out a rotating variant of it on collection ("Daily Hunting 1:
-- Weak Mortirex"). Modelling each variant as an independent quest is
-- right for analytics (each variant has its own economics) but wrong for
-- availability: completing today's variant puts the whole line on
-- cooldown, so sibling variants must read as unavailable together. A
-- family is that line: variants are members, and the family is what
-- cools.
--
-- The anchor records WHEN a cooldown timer starts, because the game and
-- the app historically disagreed: the observed daily behaviour starts
-- the timer at collection (the moment the NPC hands the mission over),
-- not at completion. 'pickup' anchors on the member's last recorded
-- start; 'completion' anchors on its last recorded completion. An
-- abandoned mission therefore keeps an honest pickup-anchored timer: the
-- start happened, whether or not a completion ever follows.
--
-- `last_started_at` exists because `started_at` is lifecycle state (it
-- clears on completion or cancel) while a pickup-anchored timer needs a
-- durable fact. It is stamped on every start and never cleared.
--
-- Additive and forward-only: no existing row changes meaning; quests
-- keep their own per-quest cooldown, anchored at completion as before,
-- and a NULL family_id leaves a quest standalone.

CREATE TABLE quest_families (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    planet TEXT NOT NULL DEFAULT 'Calypso',
    -- NULL means the family groups variants without gating them.
    cooldown_hours REAL,
    -- 'pickup' (timer runs from the member's last start) or
    -- 'completion' (from its last completion). Collection-timed daily
    -- slots are the motivating case, hence the default.
    cooldown_anchor TEXT NOT NULL DEFAULT 'pickup',
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at REAL NOT NULL DEFAULT (unixepoch('now')),
    updated_at REAL
);

ALTER TABLE quests ADD COLUMN family_id INTEGER REFERENCES quest_families(id);
ALTER TABLE quests ADD COLUMN cooldown_anchor TEXT NOT NULL DEFAULT 'completion';
ALTER TABLE quests ADD COLUMN last_started_at REAL;

-- A quest currently in progress has a live start; seed the durable stamp
-- from it. Historical starts are unrecoverable and stay NULL.
UPDATE quests SET last_started_at = started_at WHERE started_at IS NOT NULL;

CREATE INDEX idx_quests_family ON quests(family_id);
