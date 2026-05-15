"""Event type constants for the event bus."""

# ── Chat.log parser events ──

EVENT_COMBAT = "combat"
EVENT_LOOT_GROUP = "loot_group"
EVENT_SKILL_GAIN = "skill_gain"
EVENT_ENHANCER_BREAK = "enhancer_break"
EVENT_GLOBAL = "global"

# ── Hotbar / tool events ──

EVENT_ACTIVE_TOOL_CHANGED = "active_tool_changed"
EVENT_ACTIVE_HEAL_TOOL_CHANGED = "active_heal_tool_changed"

# ── Tracking session events ──

EVENT_SESSION_STARTED = "session_started"
EVENT_SESSION_STOPPED = "session_stopped"

# ── Mission events ──

EVENT_MISSION_RECEIVED = "mission_received"
