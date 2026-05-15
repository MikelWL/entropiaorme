"""In-memory data models for tracking sessions, kills, and combat."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime


@dataclass
class LootItem:
    """A single item received from a loot drop."""

    item_name: str
    quantity: int
    value_ped: float
    is_enhancer_shrapnel: bool = False


@dataclass
class ToolStats:
    """Per-tool damage statistics within a kill."""

    tool_name: str
    shots_fired: int = 0
    damage_dealt: float = 0.0
    critical_hits: int = 0
    cost_per_shot: float = 0.0  # From equipment library


@dataclass
class Kill:
    """A single kill — one loot group with its accumulated combat stats.

    Created when a loot group arrives. The accumulated shots/cost since the
    previous kill (or session start) are snapshotted into this record.
    mob_name is stamped from the current manual mob or free-text tag.
    """

    id: str
    session_id: str
    mob_name: str  # snapshot from manual/tag state; "Unknown" if unset
    mob_species: str = ""
    mob_maturity: str = ""
    timestamp: float = 0.0  # epoch — when loot arrived

    # Accumulated combat stats since last kill
    shots_fired: int = 0
    damage_dealt: float = 0.0
    damage_taken: float = 0.0
    critical_hits: int = 0
    cost_ped: float = 0.0  # total weapon cost (sum of cost_per_shot × shots per tool)

    # Enhancer cost accumulated during this kill's shots
    enhancer_cost: float = 0.0

    # Loot
    loot_total_ped: float = 0.0
    loot_items: list[LootItem] = field(default_factory=list)

    # Per-tool tracking
    tool_stats: dict[str, ToolStats] = field(default_factory=dict)

    # Notable event flags
    is_global: bool = False
    is_hof: bool = False


@dataclass
class TrackingSession:
    """A tracking session — started/stopped by the user."""

    id: str
    start_time: datetime = field(default_factory=datetime.now)
    end_time: datetime | None = None
    kills: list[Kill] = field(default_factory=list)
    dangling_cost: float = 0.0  # unresolved shots at session end
