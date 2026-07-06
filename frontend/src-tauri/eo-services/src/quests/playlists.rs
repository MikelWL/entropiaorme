//! Playlist CRUD and shaping: classified items (immediate /
//! long-horizon), membership normalisation from either payload shape,
//! and the item-group split.
//!
//! The write path parses payloads into [`PlaylistItemPayload`] at the
//! normalisation boundary: the group vocabulary is validated once,
//! there, and the insert loop consumes only well-formed items (the
//! defensive re-validation and bare-id fallbacks the loop used to
//! carry are unrepresentable now).

use rusqlite::OptionalExtension as _;
use serde_json::{json, Map, Value};

use crate::db::DbError;

use super::payload::{python_str, value_to_sql};
use super::{QuestError, QuestService};

pub const PLAYLIST_GROUP_IMMEDIATE: &str = "immediate";
pub const PLAYLIST_GROUP_LONG_HORIZON: &str = "long_horizon";

/// A playlist item's classification: the closed two-word vocabulary,
/// parsed once at the payload boundary and rendered back at the bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaylistGroup {
    Immediate,
    LongHorizon,
}

impl PlaylistGroup {
    fn as_str(self) -> &'static str {
        match self {
            PlaylistGroup::Immediate => PLAYLIST_GROUP_IMMEDIATE,
            PlaylistGroup::LongHorizon => PLAYLIST_GROUP_LONG_HORIZON,
        }
    }

    /// Parse a payload group value; the refusal renders the raw value
    /// the way the original's message does (`None` for null, verbatim
    /// text otherwise).
    fn parse(raw: &Value) -> Result<PlaylistGroup, QuestError> {
        match raw.as_str() {
            Some(text) if text == PLAYLIST_GROUP_IMMEDIATE => Ok(PlaylistGroup::Immediate),
            Some(text) if text == PLAYLIST_GROUP_LONG_HORIZON => Ok(PlaylistGroup::LongHorizon),
            _ => Err(QuestError::Invalid(format!(
                "Invalid playlist group type: {}",
                python_str(raw)
            ))),
        }
    }
}

/// One normalised playlist item on the write path. The id and
/// description stay raw payload values (they bind under the original's
/// adapter rules, whatever scalar the payload carried); the group is
/// parsed.
pub(super) struct PlaylistItemPayload {
    quest_id: Value,
    description: Value,
    group: PlaylistGroup,
}

impl QuestService {
    // ── Playlist CRUD ───────────────────────────────────────────────

    /// List all playlists with classified items in order.
    pub async fn get_playlists(&self, active_only: bool) -> Result<Vec<Value>, QuestError> {
        let where_clause = if active_only {
            "WHERE is_active = 1"
        } else {
            ""
        };
        let sql = format!(
            "SELECT id, name, planet, estimated_minutes, is_active, created_at, updated_at \
             FROM quest_playlists {where_clause} ORDER BY created_at ASC"
        );
        let bases = self
            .db
            .with_reader(move |conn| {
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push(row_to_playlist(row));
                }
                Ok(out)
            })
            .await?;
        let mut playlists = Vec::with_capacity(bases.len());
        for base in bases {
            playlists.push(Value::Object(self.shape_playlist(base).await?));
        }
        Ok(playlists)
    }

    /// A single playlist by ID; `None` when absent.
    pub async fn get_playlist(&self, playlist_id: i64) -> Result<Option<Value>, QuestError> {
        let Some(base) = self
            .db
            .with_reader(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT id, name, planet, estimated_minutes, is_active, created_at, updated_at \
                         FROM quest_playlists WHERE id = ?",
                        rusqlite::params![playlist_id],
                        |row| Ok(row_to_playlist(row)),
                    )
                    .optional()?)
            })
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(Value::Object(self.shape_playlist(base).await?)))
    }

    async fn shape_playlist(
        &self,
        mut playlist: Map<String, Value>,
    ) -> Result<Map<String, Value>, QuestError> {
        let playlist_id = playlist["id"].as_i64().expect("integer playlist id");
        let items = self.playlist_items(playlist_id).await?;
        let (immediate_ids, long_horizon_ids) = split_playlist_item_groups(&items);
        playlist.insert(
            "quest_ids".into(),
            json!(items
                .iter()
                .map(|item| item["quest_id"].clone())
                .collect::<Vec<_>>()),
        );
        playlist.insert("immediate_quest_ids".into(), json!(immediate_ids));
        playlist.insert("long_horizon_quest_ids".into(), json!(long_horizon_ids));
        playlist.insert("items".into(), json!(items));
        Ok(playlist)
    }

    /// Create a playlist with classified items.
    pub async fn create_playlist(&self, data: &Value) -> Result<Value, QuestError> {
        let items = normalize_playlist_items(data)?;
        let planet = match data.get("planet") {
            None => json!("Calypso"),
            Some(value) => value.clone(),
        };
        let estimated = match data.get("estimated_minutes") {
            None => json!(30),
            Some(value) => value.clone(),
        };
        let params: Vec<rusqlite::types::Value> = vec![
            value_to_sql(data.get("name").expect("playlist payload carries name")),
            value_to_sql(&planet),
            value_to_sql(&estimated),
        ];
        let playlist_id = self
            .db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO quest_playlists (name, planet, estimated_minutes) VALUES (?, ?, ?)",
                    rusqlite::params_from_iter(params),
                )?;
                let playlist_id = tx.last_insert_rowid();
                set_playlist_items(&tx, playlist_id, &items)?;
                tx.commit()?;
                Ok(playlist_id)
            })
            .await?;

        Ok(self
            .get_playlist(playlist_id)
            .await?
            .expect("the playlist was just inserted"))
    }

    /// Update a playlist's fields and/or classified quest groups;
    /// `None` when absent.
    pub async fn update_playlist(
        &self,
        playlist_id: i64,
        data: &Value,
    ) -> Result<Option<Value>, QuestError> {
        if self.get_playlist(playlist_id).await?.is_none() {
            return Ok(None);
        }

        const ALLOWED: [&str; 3] = ["name", "planet", "estimated_minutes"];
        let updates: Vec<(&str, &Value)> = ALLOWED
            .iter()
            .filter_map(|key| data.get(*key).map(|value| (*key, value)))
            .collect();

        let replace_items = data.get("items").is_some() || data.get("quest_ids").is_some();
        let items = if replace_items {
            Some(normalize_playlist_items(data)?)
        } else {
            None
        };

        let update = (!updates.is_empty()).then(|| {
            let set_clause = updates
                .iter()
                .map(|(key, _)| format!("{key} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("UPDATE quest_playlists SET {set_clause} WHERE id = ?");
            let mut params: Vec<rusqlite::types::Value> = updates
                .iter()
                .map(|(_, value)| value_to_sql(value))
                .collect();
            params.push(rusqlite::types::Value::Integer(playlist_id));
            (sql, params)
        });

        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                if let Some((sql, params)) = update {
                    tx.execute(&sql, rusqlite::params_from_iter(params))?;
                }
                if let Some(items) = &items {
                    set_playlist_items(&tx, playlist_id, items)?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?;

        self.get_playlist(playlist_id).await
    }

    /// Soft-delete a playlist and clear its items.
    pub async fn delete_playlist(&self, playlist_id: i64) -> Result<bool, QuestError> {
        let affected = self
            .db
            .with_writer(move |conn| {
                Ok(conn.execute(
                    "UPDATE quest_playlists SET is_active = 0 WHERE id = ? AND is_active = 1",
                    rusqlite::params![playlist_id],
                )?)
            })
            .await?;
        if affected > 0 {
            self.db
                .with_writer(move |conn| {
                    conn.execute(
                        "DELETE FROM quest_playlist_items WHERE playlist_id = ?",
                        rusqlite::params![playlist_id],
                    )?;
                    Ok(())
                })
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) async fn quest_playlist_ids(&self, quest_id: i64) -> Result<Vec<i64>, QuestError> {
        Ok(self
            .db
            .with_reader(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT qpi.playlist_id FROM quest_playlist_items qpi \
                     JOIN quest_playlists qp ON qp.id = qpi.playlist_id \
                     WHERE qpi.quest_id = ? AND qp.is_active = 1",
                )?;
                let mut rows = stmt.query(rusqlite::params![quest_id])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push(row.get::<_, i64>(0)?);
                }
                Ok(out)
            })
            .await?)
    }

    async fn playlist_items(&self, playlist_id: i64) -> Result<Vec<Value>, QuestError> {
        Ok(self
            .db
            .with_reader(move |conn| {
                // Immediate items sort ahead of long-horizon ones (the
                // boolean expression), then by their explicit order.
                let mut stmt = conn.prepare(
                    "SELECT quest_id, description, group_type \
                     FROM quest_playlist_items \
                     WHERE playlist_id = ? \
                     ORDER BY group_type = ?, sort_order",
                )?;
                let mut rows =
                    stmt.query(rusqlite::params![playlist_id, PLAYLIST_GROUP_LONG_HORIZON])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push(json!({
                        "quest_id": row.get::<_, i64>(0)?,
                        "description": row.get::<_, Option<String>>(1)?,
                        "group_type": row.get::<_, String>(2)?,
                    }));
                }
                Ok(out)
            })
            .await?)
    }
}

fn row_to_playlist(row: &rusqlite::Row) -> Map<String, Value> {
    let mut playlist = Map::new();
    playlist.insert("id".into(), json!(row.get_unwrap::<_, i64>("id")));
    playlist.insert("name".into(), json!(row.get_unwrap::<_, String>("name")));
    playlist.insert(
        "planet".into(),
        json!(row.get_unwrap::<_, String>("planet")),
    );
    playlist.insert(
        "estimated_minutes".into(),
        json!(row.get_unwrap::<_, i64>("estimated_minutes")),
    );
    playlist.insert(
        "is_active".into(),
        json!(row.get_unwrap::<_, i64>("is_active")),
    );
    playlist.insert(
        "created_at".into(),
        json!(row.get_unwrap::<_, f64>("created_at")),
    );
    playlist.insert(
        "updated_at".into(),
        json!(row.get_unwrap::<_, Option<f64>>("updated_at")),
    );
    playlist
}

/// Rewrite a playlist's items with explicit grouping. The items arrive
/// parsed (the group vocabulary validated at the normalisation
/// boundary); the caller's enclosing transaction rolls the rewrite
/// back whole on any error, preserving the original's
/// nothing-committed refusal shape.
fn set_playlist_items(
    conn: &rusqlite::Connection,
    playlist_id: i64,
    items: &[PlaylistItemPayload],
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM quest_playlist_items WHERE playlist_id = ?",
        rusqlite::params![playlist_id],
    )?;
    for (index, item) in items.iter().enumerate() {
        conn.execute(
            "INSERT INTO quest_playlist_items \
             (playlist_id, quest_id, sort_order, description, group_type) \
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![
                playlist_id,
                value_to_sql(&item.quest_id),
                index as i64,
                value_to_sql(&item.description),
                item.group.as_str()
            ],
        )?;
    }
    Ok(())
}

/// Normalise playlist payloads to classified items: an `items` list
/// passes through with group defaults; otherwise `quest_ids` builds
/// immediate items. A present-but-null `items` falls through to the
/// `quest_ids` leg exactly as the original's is-not-None test does,
/// so `{"items": null}` alone clears the playlist (the original's
/// semantics, pinned). A present-but-null (or non-list) `quest_ids`
/// refuses instead: the original crashes iterating it (an unhandled
/// error on the wire, with no surviving write), and the update path
/// is reachable with an explicit null through the route model.
///
/// The group vocabulary is validated here, at the boundary; the
/// original validated inside the insert loop, but every refusal rolls
/// back whole either way, so the observable outcome (the verbatim
/// message, no surviving write) is identical.
fn normalize_playlist_items(data: &Value) -> Result<Vec<PlaylistItemPayload>, QuestError> {
    if let Some(items) = data.get("items").filter(|value| !value.is_null()) {
        return items
            .as_array()
            .expect("items is a list")
            .iter()
            .map(|item| {
                let group = match item.get("group_type") {
                    None => PlaylistGroup::Immediate,
                    Some(raw) => PlaylistGroup::parse(raw)?,
                };
                Ok(PlaylistItemPayload {
                    quest_id: item.get("quest_id").expect("item carries quest_id").clone(),
                    description: item.get("description").cloned().unwrap_or(Value::Null),
                    group,
                })
            })
            .collect();
    }
    let quest_ids = match data.get("quest_ids") {
        None => &[] as &[Value],
        Some(value) => value
            .as_array()
            .ok_or_else(|| {
                QuestError::Invalid("'quest_ids' must be a list of quest ids".to_string())
            })?
            .as_slice(),
    };
    Ok(quest_ids
        .iter()
        .map(|quest_id| PlaylistItemPayload {
            quest_id: quest_id.clone(),
            description: Value::Null,
            group: PlaylistGroup::Immediate,
        })
        .collect())
}

/// Partition item quest ids by group: everything not long-horizon is
/// immediate.
fn split_playlist_item_groups(items: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let group = |item: &Value| {
        item.get("group_type")
            .and_then(Value::as_str)
            .map(String::from)
    };
    let immediate = items
        .iter()
        .filter(|item| group(item).as_deref() != Some(PLAYLIST_GROUP_LONG_HORIZON))
        .map(|item| item["quest_id"].clone())
        .collect();
    let long_horizon = items
        .iter()
        .filter(|item| group(item).as_deref() == Some(PLAYLIST_GROUP_LONG_HORIZON))
        .map(|item| item["quest_id"].clone())
        .collect();
    (immediate, long_horizon)
}
