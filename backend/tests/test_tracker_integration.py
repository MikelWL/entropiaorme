"""Integration tests: full tracking pipeline via event bus.

Tests the kills model: shots accumulate, loot creates kill records,
session end creates dangling cost.
"""

import sqlite3
import time
import uuid
from datetime import datetime, timedelta

import pytest

from backend.core.event_bus import EventBus
from backend.core.events import (
    EVENT_COMBAT,
    EVENT_ENHANCER_BREAK,
    EVENT_GLOBAL,
    EVENT_LOOT_GROUP,
)
from backend.tracking.tracker import HuntTracker
from backend.tracking.schema import init_tracking_tables


@pytest.fixture
def pipeline():
    """Set up a full tracking pipeline with in-memory DB."""
    db = sqlite3.connect(":memory:", check_same_thread=False)
    bus = EventBus()
    tracker = HuntTracker(bus, db)
    return bus, tracker, db


class TestFullPipeline:
    def test_start_stop_empty_session(self, pipeline):
        bus, tracker, db = pipeline
        session = tracker.start_session()
        assert tracker.is_tracking
        assert session.id

        result = tracker.stop_session()
        assert not tracker.is_tracking
        assert result.end_time is not None
        assert len(result.kills) == 0

        # Verify DB
        row = db.execute(
            "SELECT is_active FROM tracking_sessions WHERE id = ?",
            (session.id,),
        ).fetchone()
        assert row[0] == 0

    def test_combat_accumulates_stats(self, pipeline):
        """Combat without loot → no kills, stats in dangling cost."""
        bus, tracker, db = pipeline
        session = tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.5, "timestamp": now})
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 15.0, "timestamp": now})
        bus.publish(EVENT_COMBAT, {"type": "critical_hit", "amount": 30.0, "timestamp": now})

        # Accumulator should have stats
        acc = tracker.current_accumulator
        assert acc.shots_fired == 3
        assert acc.damage_dealt == 55.5
        assert acc.critical_hits == 1

        result = tracker.stop_session()
        assert len(result.kills) == 0
        # Dangling cost persisted (weapon cost = 0 since no equipment lookup)
        assert result.dangling_cost == 0.0  # No cost_per_shot configured

    def test_loot_creates_kill(self, pipeline):
        """Damage then loot → kill record created with correct stats."""
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 15.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "type": "loot",
            "items": [
                {"item_name": "Shrapnel", "quantity": 50, "value_ped": 0.50},
                {"item_name": "Animal Oil Residue", "quantity": 3, "value_ped": 0.03},
            ],
            "total_ped": 0.53,
            "timestamp": now,
        })

        result = tracker.stop_session()
        assert len(result.kills) == 1

        kill = result.kills[0]
        assert kill.loot_total_ped == 0.53
        assert kill.shots_fired == 2
        assert kill.damage_dealt == 25.0

        # Check loot in DB
        loot_rows = db.execute(
            "SELECT item_name, quantity, value_ped FROM kill_loot_items WHERE kill_id = ?",
            (kill.id,),
        ).fetchall()
        assert len(loot_rows) == 2
        names = {r[0] for r in loot_rows}
        assert "Shrapnel" in names

    def test_accumulator_resets_on_loot(self, pipeline):
        """After a kill, accumulator starts fresh."""
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Shrapnel", "quantity": 1, "value_ped": 0.01}],
            "total_ped": 0.01,
            "timestamp": now,
        })

        # Accumulator should be reset
        acc = tracker.current_accumulator
        assert acc.shots_fired == 0
        assert acc.damage_dealt == 0.0

        tracker.stop_session()

    def test_multiple_kills_in_session(self, pipeline):
        """Multiple loot events create multiple kill records."""
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        for i in range(3):
            t = now + timedelta(seconds=i * 5)
            bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": t})
            bus.publish(EVENT_LOOT_GROUP, {
                "items": [{"item_name": "Shrapnel", "quantity": 10, "value_ped": 0.10}],
                "total_ped": 0.10,
                "timestamp": t + timedelta(seconds=1),
            })

        result = tracker.stop_session()
        assert len(result.kills) == 3

        # Verify DB
        db_kills = db.execute(
            "SELECT id FROM kills WHERE session_id = ?", (result.id,),
        ).fetchall()
        assert len(db_kills) == 3

    def test_dangling_cost_with_equipment(self, pipeline):
        """Shots without loot → dangling cost includes weapon cost."""
        db = sqlite3.connect(":memory:")
        bus = EventBus()
        # Equipment lookup returns cost per shot
        tracker = HuntTracker(bus, db, equipment_cost_lookup=lambda _: 0.50)
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 15.0, "timestamp": now})

        result = tracker.stop_session()
        assert len(result.kills) == 0
        assert abs(result.dangling_cost - 1.00) < 1e-6  # 2 shots × 0.50

        # Verify in DB
        row = db.execute(
            "SELECT dangling_cost FROM tracking_sessions WHERE id = ?", (result.id,),
        ).fetchone()
        assert abs(row[0] - 1.00) < 1e-6

    def test_countered_shot_counts_cost_in_standard_mode(self, pipeline):
        """Countered attacks still consume a shot and cost in standard mode."""
        db = sqlite3.connect(":memory:")
        bus = EventBus()
        tracker = HuntTracker(bus, db, equipment_cost_lookup=lambda _: 0.50)
        tracker.start_session()

        from backend.core.events import EVENT_ACTIVE_TOOL_CHANGED
        now = datetime.now(tz=None)
        bus.publish(EVENT_ACTIVE_TOOL_CHANGED, {"tool_name": "Opalo"})
        bus.publish(EVENT_COMBAT, {"type": "target_jam", "timestamp": now})

        result = tracker.stop_session()
        assert result.dangling_cost == 0.50

        row = db.execute(
            "SELECT dangling_cost FROM tracking_sessions WHERE id = ?", (result.id,),
        ).fetchone()
        assert abs(row[0] - 0.50) < 1e-6

    def test_blacklisted_loot_filtered(self, pipeline):
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Universal Ammo", "quantity": 100, "value_ped": 1.0}],
            "total_ped": 1.0,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]
        assert len(kill.loot_items) == 0  # Universal Ammo is blacklisted
        assert kill.loot_total_ped == 0.0

    def test_blacklist_refreshes_before_session_start(self):
        db = sqlite3.connect(":memory:", check_same_thread=False)
        bus = EventBus()
        blacklist = ["Universal Ammo"]
        tracker = HuntTracker(
            bus,
            db,
            loot_filter_blacklist=blacklist.copy(),
            loot_filter_blacklist_provider=lambda: blacklist,
        )

        blacklist.append("Animal Oil Residue")
        tracker.reload_config()
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [
                {"item_name": "Animal Oil Residue", "quantity": 3, "value_ped": 0.03},
                {"item_name": "Shrapnel", "quantity": 5, "value_ped": 0.05},
            ],
            "total_ped": 0.08,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]
        assert len(kill.loot_items) == 1
        assert kill.loot_items[0].item_name == "Shrapnel"
        assert kill.loot_total_ped == 0.05

    def test_unknown_mob_when_no_lock(self, pipeline):
        """Kill gets 'Unknown' mob when no manual mob/tag is set."""
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Shrapnel", "quantity": 1, "value_ped": 0.01}],
            "total_ped": 0.01,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]
        assert kill.mob_name == "Unknown"

    def test_tool_stats_merge_unknown(self, pipeline):
        """Unknown tool stats merge into real tool on detection."""
        bus, tracker, db = pipeline
        tracker.start_session()

        from backend.core.events import EVENT_ACTIVE_TOOL_CHANGED
        now = datetime.now(tz=None)

        # Shots before tool detection go to "Unknown"
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 15.0, "timestamp": now})

        # Tool detected → merge
        bus.publish(EVENT_ACTIVE_TOOL_CHANGED, {"tool_name": "Opalo"})

        # More shots under real tool
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 20.0, "timestamp": now})

        # Loot → creates kill
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Shrapnel", "quantity": 1, "value_ped": 0.01}],
            "total_ped": 0.01,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]
        assert "Unknown" not in kill.tool_stats
        assert "Opalo" in kill.tool_stats
        assert kill.tool_stats["Opalo"].shots_fired == 3
        assert kill.tool_stats["Opalo"].damage_dealt == 45.0

    def test_shrapnel_conversion_ledger_entry(self, pipeline):
        """Shrapnel looted during a session creates a 1% margin ledger entry."""
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [
                {"item_name": "Shrapnel", "quantity": 1000, "value_ped": 10.00},
                {"item_name": "Animal Oil Residue", "quantity": 5, "value_ped": 0.05},
            ],
            "total_ped": 10.05,
            "timestamp": now,
        })

        tracker.stop_session()

        ledger = db.execute(
            "SELECT type, description, amount, tag FROM ledger_entries"
        ).fetchall()
        assert len(ledger) == 1
        entry = ledger[0]
        assert entry[0] == "markup"
        assert entry[1] == "Shrapnel Conversion"
        assert abs(entry[2] - 0.10) < 0.001
        assert entry[3] == "convert"

    def test_enhancer_refund_creates_rebate_and_skips_conversion_margin(self, pipeline):
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_ENHANCER_BREAK, {
            "enhancer_name": "T1 Weapon Damage Enhancer",
            "item_name": "Electric Attack Nanochip 9",
            "remaining": 3,
            "shrapnel_ped": 0.80,
        })
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [
                {
                    "item_name": "Shrapnel",
                    "quantity": 8000,
                    "value_ped": 0.80,
                    "is_enhancer_shrapnel": True,
                },
                {"item_name": "Shrapnel", "quantity": 1000, "value_ped": 10.00},
            ],
            "total_ped": 10.80,
            "timestamp": now,
        })

        result = tracker.stop_session()

        assert len(result.kills) == 1
        assert result.kills[0].loot_total_ped == 10.00

        ledger = db.execute(
            "SELECT description, amount, tag FROM ledger_entries ORDER BY description"
        ).fetchall()
        assert ledger == [
            ("Enhancer Shrapnel Rebate", 0.8, "enhancer"),
            ("Shrapnel Conversion", 0.1, "convert"),
        ]

    def test_no_shrapnel_no_ledger_entry(self, pipeline):
        bus, tracker, db = pipeline
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Animal Oil Residue", "quantity": 5, "value_ped": 0.05}],
            "total_ped": 0.05,
            "timestamp": now,
        })

        tracker.stop_session()

        ledger = db.execute("SELECT * FROM ledger_entries").fetchall()
        assert len(ledger) == 0

    def test_schema_created(self, pipeline):
        _, _, db = pipeline
        tables = db.execute(
            "SELECT name FROM sqlite_master WHERE type='table'",
        ).fetchall()
        table_names = {t[0] for t in tables}
        assert "tracking_sessions" in table_names
        assert "kills" in table_names
        assert "kill_tool_stats" in table_names
        assert "kill_loot_items" in table_names
        assert "ledger_entries" in table_names
        loot_cols = {row[1] for row in db.execute("PRAGMA table_info(kill_loot_items)").fetchall()}
        assert "is_enhancer_shrapnel" in loot_cols


# ── Mob Lock Confirmation & Retrofit ──────────────────────────────────

_kill_counter = 0


def _make_kill(bus, now=None):
    """Publish combat + loot to create a kill record.

    Uses a counter to vary total_ped slightly, avoiding loot deduplication.
    """
    global _kill_counter
    _kill_counter += 1
    now = now or datetime.now(tz=None)
    ped = 0.01 * _kill_counter
    bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 10.0, "timestamp": now})
    bus.publish(EVENT_LOOT_GROUP, {
        "items": [{"item_name": "Shrapnel", "quantity": _kill_counter, "value_ped": ped}],
        "total_ped": ped,
        "timestamp": now,
    })


class TestTrifectaInferredManualMobLock:
    def test_prearmed_trifecta_inferred_mob_is_active_from_session_start(self):
        db = sqlite3.connect(":memory:", check_same_thread=False)
        bus = EventBus()
        tracker = HuntTracker(
            bus,
            db,
            weapon_attribution_trifecta_provider=lambda: True,
            manual_mob_provider=lambda: ("Atrox", "Young"),
        )
        tracker.start_session()

        _make_kill(bus)
        result = tracker.stop_session()

        assert result.kills[0].mob_name == "Young Atrox"
        assert result.kills[0].mob_species == "Atrox"
        assert result.kills[0].mob_maturity == "Young"

    def test_prearmed_standard_manual_mob_is_active_from_session_start(self):
        db = sqlite3.connect(":memory:", check_same_thread=False)
        bus = EventBus()
        tracker = HuntTracker(
            bus,
            db,
            weapon_attribution_trifecta_provider=lambda: False,
            manual_mob_entry_enabled_provider=lambda: True,
            manual_mob_provider=lambda: ("Atrox", "Young"),
        )
        tracker.start_session()

        _make_kill(bus)
        result = tracker.stop_session()

        assert result.kills[0].mob_name == "Young Atrox"
        assert result.kills[0].mob_species == "Atrox"
        assert result.kills[0].mob_maturity == "Young"

    def test_manual_lock_stamps_future_kills_without_retrofit(self, pipeline):
        bus, tracker, db = pipeline
        tracker.start_session()

        _make_kill(bus)
        assert tracker.session.kills[0].mob_name == "Unknown"

        tracker.set_manual_mob("Young Atrox", "Atrox", "Young")
        _make_kill(bus)

        result = tracker.stop_session()
        assert [kill.mob_name for kill in result.kills] == ["Unknown", "Young Atrox"]
        assert result.kills[1].mob_species == "Atrox"
        assert result.kills[1].mob_maturity == "Young"

    def test_manual_release_returns_to_unknown(self, pipeline):
        bus, tracker, db = pipeline
        tracker.start_session()

        tracker.set_manual_mob("Young Atrox", "Atrox", "Young")
        _make_kill(bus)
        assert tracker.release_current_mob() == "Young Atrox"
        _make_kill(bus)

        result = tracker.stop_session()
        assert [kill.mob_name for kill in result.kills] == ["Young Atrox", "Unknown"]


class TestSessionTagMode:
    def test_prearmed_tag_is_active_from_session_start(self):
        db = sqlite3.connect(":memory:", check_same_thread=False)
        bus = EventBus()
        tracker = HuntTracker(
            bus,
            db,
            mob_tracking_mode_provider=lambda: "tag",
            mob_tracking_tag_provider=lambda: "Easter Mayhem",
        )
        tracker.start_session()

        _make_kill(bus)
        result = tracker.stop_session()

        assert result.kills[0].mob_name == "Easter Mayhem"
        assert result.kills[0].mob_species == ""
        assert result.kills[0].mob_maturity == ""

    def test_tag_mode_defaults_to_unknown_until_tag_is_set(self):
        db = sqlite3.connect(":memory:", check_same_thread=False)
        bus = EventBus()
        tracker = HuntTracker(
            bus,
            db,
            mob_tracking_mode_provider=lambda: "tag",
            mob_tracking_tag_provider=lambda: "",
        )
        tracker.start_session()

        _make_kill(bus)
        result = tracker.stop_session()

        assert result.kills[0].mob_name == "Unknown"
        assert result.kills[0].mob_species == ""
        assert result.kills[0].mob_maturity == ""

    def test_tag_mode_stamps_future_kills_after_tag_is_set(self):
        db = sqlite3.connect(":memory:", check_same_thread=False)
        bus = EventBus()
        tracker = HuntTracker(
            bus,
            db,
            mob_tracking_mode_provider=lambda: "tag",
            mob_tracking_tag_provider=lambda: "",
        )
        tracker.start_session()
        tracker.set_manual_tag("Easter Mayhem")

        _make_kill(bus)
        result = tracker.stop_session()

        assert result.kills[0].mob_name == "Easter Mayhem"
        assert result.kills[0].mob_species == ""
        assert result.kills[0].mob_maturity == ""

    def test_tag_release_returns_to_unknown(self):
        db = sqlite3.connect(":memory:", check_same_thread=False)
        bus = EventBus()
        tracker = HuntTracker(
            bus,
            db,
            mob_tracking_mode_provider=lambda: "tag",
            mob_tracking_tag_provider=lambda: "",
        )
        tracker.start_session()

        tracker.set_manual_tag("Easter Mayhem")
        _make_kill(bus)
        assert tracker.release_current_mob() == "Easter Mayhem"
        _make_kill(bus)

        result = tracker.stop_session()
        assert [kill.mob_name for kill in result.kills] == ["Easter Mayhem", "Unknown"]


# ── Global/HoF correlation ──────────────────────────────────────────────

@pytest.fixture
def pipeline_with_player():
    """Pipeline with player_name set; needed for global event filtering."""
    db = sqlite3.connect(":memory:", check_same_thread=False)
    bus = EventBus()
    tracker = HuntTracker(bus, db, player_name="TestPlayer")
    return bus, tracker, db


class TestGlobalCorrelation:
    def test_global_flags_recent_kill(self, pipeline_with_player):
        """Global event correlates to the most recently created kill."""
        bus, tracker, db = pipeline_with_player
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 50.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [
                {"item_name": "Shrapnel", "quantity": 5000, "value_ped": 50.00},
                {"item_name": "Blazar Fragment", "quantity": 1, "value_ped": 2.50},
            ],
            "total_ped": 52.50,
            "timestamp": now,
        })

        bus.publish(EVENT_GLOBAL, {
            "type": "global_kill",
            "player": "TestPlayer",
            "creature": "Atrox Provider",
            "value": 52.50,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]
        assert kill.is_global is True
        assert kill.is_hof is False

        row = db.execute(
            "SELECT is_global, is_hof FROM kills WHERE id = ?", (kill.id,)
        ).fetchone()
        assert row[0] == 1
        assert row[1] == 0

    def test_hof_flags_kill(self, pipeline_with_player):
        """HoF event sets both is_global and is_hof."""
        bus, tracker, db = pipeline_with_player
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 100.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Shrapnel", "quantity": 10000, "value_ped": 100.00}],
            "total_ped": 100.00,
            "timestamp": now,
        })
        bus.publish(EVENT_GLOBAL, {
            "type": "hof_kill",
            "player": "TestPlayer",
            "creature": "Atrox Stalker",
            "value": 100.00,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]
        assert kill.is_global is True
        assert kill.is_hof is True

    def test_other_player_global_ignored(self, pipeline_with_player):
        bus, tracker, db = pipeline_with_player
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 50.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Shrapnel", "quantity": 5000, "value_ped": 50.00}],
            "total_ped": 50.00,
            "timestamp": now,
        })
        bus.publish(EVENT_GLOBAL, {
            "type": "global_kill",
            "player": "SomeoneElse",
            "creature": "Atrox",
            "value": 50.00,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]
        assert kill.is_global is False

    def test_notable_event_has_kill_id(self, pipeline_with_player):
        """Notable event record has the correct kill_id."""
        bus, tracker, db = pipeline_with_player
        tracker.start_session()

        now = datetime.now(tz=None)
        bus.publish(EVENT_COMBAT, {"type": "damage_dealt", "amount": 30.0, "timestamp": now})
        bus.publish(EVENT_LOOT_GROUP, {
            "items": [{"item_name": "Shrapnel", "quantity": 3000, "value_ped": 30.00}],
            "total_ped": 30.00,
            "timestamp": now,
        })
        bus.publish(EVENT_GLOBAL, {
            "type": "global_kill",
            "player": "TestPlayer",
            "creature": "Atrox",
            "value": 30.00,
            "timestamp": now,
        })

        result = tracker.stop_session()
        kill = result.kills[0]

        notable = db.execute(
            "SELECT kill_id FROM notable_events WHERE event_type = 'global_kill'"
        ).fetchone()
        assert notable[0] == kill.id

# ── Crash recovery ─────────────────────────────────────────────────────

def _setup_orphan_db():
    """Create a DB with tracking tables and return it; no HuntTracker yet."""
    db = sqlite3.connect(":memory:")
    init_tracking_tables(db)
    return db


class TestCrashRecovery:
    def test_orphaned_session_with_kills_recovered(self):
        """Orphaned session with persisted kills is closed with correct end time."""
        db = _setup_orphan_db()
        session_id = str(uuid.uuid4())
        started_at = time.time() - 3600

        db.execute(
            "INSERT INTO tracking_sessions (id, started_at, is_active) VALUES (?, ?, 1)",
            (session_id, started_at),
        )
        # Two persisted kills
        kill1_id = str(uuid.uuid4())
        kill1_ts = started_at + 60
        db.execute(
            """INSERT INTO kills (id, session_id, mob_name, timestamp,
               shots_fired, damage_dealt, loot_total_ped, cost_ped)
               VALUES (?, ?, 'Atrox', ?, 10, 100.0, 1.50, 5.0)""",
            (kill1_id, session_id, kill1_ts),
        )
        kill2_id = str(uuid.uuid4())
        kill2_ts = started_at + 180
        db.execute(
            """INSERT INTO kills (id, session_id, mob_name, timestamp,
               shots_fired, damage_dealt, loot_total_ped, cost_ped)
               VALUES (?, ?, 'Atrox', ?, 20, 200.0, 3.00, 10.0)""",
            (kill2_id, session_id, kill2_ts),
        )
        # Add shrapnel loot for ledger tests
        db.execute(
            "INSERT INTO kill_loot_items (kill_id, item_name, quantity, value_ped, is_enhancer_shrapnel) VALUES (?, 'Shrapnel', 500, 5.00, 0)",
            (kill1_id,),
        )
        db.execute(
            "INSERT INTO kill_loot_items (kill_id, item_name, quantity, value_ped, is_enhancer_shrapnel) VALUES (?, 'Shrapnel', 80, 0.80, 1)",
            (kill2_id,),
        )
        db.commit()

        bus = EventBus()
        HuntTracker(bus, db)

        row = db.execute(
            "SELECT is_active, ended_at FROM tracking_sessions WHERE id = ?",
            (session_id,),
        ).fetchone()
        assert row[0] == 0
        assert row[1] == kill2_ts  # ended_at = latest kill timestamp

        ledger = db.execute(
            "SELECT description, amount, tag FROM ledger_entries ORDER BY description"
        ).fetchall()
        assert ledger == [
            ("Enhancer Shrapnel Rebate", 0.8, "enhancer"),
            ("Shrapnel Conversion", 0.05, "convert"),
        ]

    def test_orphaned_session_no_kills(self):
        """Orphaned session with no kills closes with started_at as end time."""
        db = _setup_orphan_db()
        session_id = str(uuid.uuid4())
        started_at = time.time() - 600

        db.execute(
            "INSERT INTO tracking_sessions (id, started_at, is_active) VALUES (?, ?, 1)",
            (session_id, started_at),
        )
        db.commit()

        bus = EventBus()
        HuntTracker(bus, db)

        row = db.execute(
            "SELECT is_active, ended_at FROM tracking_sessions WHERE id = ?",
            (session_id,),
        ).fetchone()
        assert row[0] == 0
        assert row[1] == started_at

    def test_already_closed_session_untouched(self):
        db = _setup_orphan_db()
        session_id = str(uuid.uuid4())
        started_at = time.time() - 3600
        ended_at = started_at + 1800

        db.execute(
            "INSERT INTO tracking_sessions (id, started_at, ended_at, is_active, heal_cost) VALUES (?, ?, ?, 0, 1.5)",
            (session_id, started_at, ended_at),
        )
        db.commit()

        bus = EventBus()
        HuntTracker(bus, db)

        row = db.execute(
            "SELECT is_active, ended_at, heal_cost FROM tracking_sessions WHERE id = ?",
            (session_id,),
        ).fetchone()
        assert row[0] == 0
        assert row[1] == ended_at
        assert row[2] == 1.5

    def test_new_session_after_recovery(self):
        db = _setup_orphan_db()
        old_id = str(uuid.uuid4())
        db.execute(
            "INSERT INTO tracking_sessions (id, started_at, is_active) VALUES (?, ?, 1)",
            (old_id, time.time() - 600),
        )
        db.commit()

        bus = EventBus()
        tracker = HuntTracker(bus, db)

        assert db.execute(
            "SELECT is_active FROM tracking_sessions WHERE id = ?", (old_id,),
        ).fetchone()[0] == 0

        session = tracker.start_session()
        assert tracker.is_tracking
        tracker.stop_session()
        assert not tracker.is_tracking
