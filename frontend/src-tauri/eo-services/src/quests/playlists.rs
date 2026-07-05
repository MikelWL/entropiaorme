//! Playlist CRUD and shaping: classified items (immediate /
//! long-horizon), membership normalisation from either payload shape,
//! and the item-group split.

use serde_json::{json, Map, Value};
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

use super::payload::{bind_json, python_str};
use super::{QuestError, QuestService};

pub const PLAYLIST_GROUP_IMMEDIATE: &str = "immediate";
pub const PLAYLIST_GROUP_LONG_HORIZON: &str = "long_horizon";

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
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(self.db.read())
            .await?;
        let mut playlists = Vec::with_capacity(rows.len());
        for row in rows {
            playlists.push(Value::Object(self.shape_playlist(&row).await?));
        }
        Ok(playlists)
    }

    /// A single playlist by ID; `None` when absent.
    pub async fn get_playlist(&self, playlist_id: i64) -> Result<Option<Value>, QuestError> {
        let Some(row) = sqlx::query(
            "SELECT id, name, planet, estimated_minutes, is_active, created_at, updated_at \
             FROM quest_playlists WHERE id = ?",
        )
        .bind(playlist_id)
        .fetch_optional(self.db.read())
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(Value::Object(self.shape_playlist(&row).await?)))
    }

    async fn shape_playlist(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<Map<String, Value>, QuestError> {
        let mut playlist = row_to_playlist(row);
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
        let mut tx = self.db.write().begin().await?;
        let query = sqlx::query(
            "INSERT INTO quest_playlists (name, planet, estimated_minutes) VALUES (?, ?, ?)",
        );
        let planet = match data.get("planet") {
            None => json!("Calypso"),
            Some(value) => value.clone(),
        };
        let estimated = match data.get("estimated_minutes") {
            None => json!(30),
            Some(value) => value.clone(),
        };
        let query = bind_json(
            query,
            data.get("name").expect("playlist payload carries name"),
        );
        let query = bind_json(query, &planet);
        let query = bind_json(query, &estimated);
        let playlist_id = query.execute(&mut *tx).await?.last_insert_rowid();
        set_playlist_items(&mut tx, playlist_id, &items).await?;
        tx.commit().await?;

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

        let mut tx = self.db.write().begin().await?;
        if !updates.is_empty() {
            let set_clause = updates
                .iter()
                .map(|(key, _)| format!("{key} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("UPDATE quest_playlists SET {set_clause} WHERE id = ?");
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for (_, value) in &updates {
                query = bind_json(query, value);
            }
            query.bind(playlist_id).execute(&mut *tx).await?;
        }
        if let Some(items) = items {
            set_playlist_items(&mut tx, playlist_id, &items).await?;
        }
        tx.commit().await?;

        self.get_playlist(playlist_id).await
    }

    /// Soft-delete a playlist and clear its items.
    pub async fn delete_playlist(&self, playlist_id: i64) -> Result<bool, QuestError> {
        let affected =
            sqlx::query("UPDATE quest_playlists SET is_active = 0 WHERE id = ? AND is_active = 1")
                .bind(playlist_id)
                .execute(self.db.write())
                .await?
                .rows_affected();
        if affected > 0 {
            sqlx::query("DELETE FROM quest_playlist_items WHERE playlist_id = ?")
                .bind(playlist_id)
                .execute(self.db.write())
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) async fn quest_playlist_ids(&self, quest_id: i64) -> Result<Vec<i64>, QuestError> {
        let rows = sqlx::query(
            "SELECT DISTINCT qpi.playlist_id FROM quest_playlist_items qpi \
             JOIN quest_playlists qp ON qp.id = qpi.playlist_id \
             WHERE qpi.quest_id = ? AND qp.is_active = 1",
        )
        .bind(quest_id)
        .fetch_all(self.db.read())
        .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    async fn playlist_items(&self, playlist_id: i64) -> Result<Vec<Value>, QuestError> {
        // Immediate items sort ahead of long-horizon ones (the boolean
        // expression), then by their explicit order.
        let rows = sqlx::query(
            "SELECT quest_id, description, group_type \
             FROM quest_playlist_items \
             WHERE playlist_id = ? \
             ORDER BY group_type = ?, sort_order",
        )
        .bind(playlist_id)
        .bind(PLAYLIST_GROUP_LONG_HORIZON)
        .fetch_all(self.db.read())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "quest_id": row.get::<i64, _>(0),
                    "description": row.get::<Option<String>, _>(1),
                    "group_type": row.get::<String, _>(2),
                })
            })
            .collect())
    }
}

fn row_to_playlist(row: &sqlx::sqlite::SqliteRow) -> Map<String, Value> {
    let mut playlist = Map::new();
    playlist.insert("id".into(), json!(row.get::<i64, _>("id")));
    playlist.insert("name".into(), json!(row.get::<String, _>("name")));
    playlist.insert("planet".into(), json!(row.get::<String, _>("planet")));
    playlist.insert(
        "estimated_minutes".into(),
        json!(row.get::<i64, _>("estimated_minutes")),
    );
    playlist.insert("is_active".into(), json!(row.get::<i64, _>("is_active")));
    playlist.insert("created_at".into(), json!(row.get::<f64, _>("created_at")));
    playlist.insert(
        "updated_at".into(),
        json!(row.get::<Option<f64>, _>("updated_at")),
    );
    playlist
}

/// Rewrite a playlist's items with explicit grouping. The original
/// validates each item inside the loop, after its delete; an invalid
/// group raises there with nothing committed, and this port's enclosing
/// transaction rolls the partial rewrite back on the same error.
async fn set_playlist_items(
    conn: &mut SqliteConnection,
    playlist_id: i64,
    items: &[Value],
) -> Result<(), QuestError> {
    sqlx::query("DELETE FROM quest_playlist_items WHERE playlist_id = ?")
        .bind(playlist_id)
        .execute(&mut *conn)
        .await?;
    for (index, item) in items.iter().enumerate() {
        let (quest_id, description, group_type) = match item {
            Value::Object(entry) => (
                entry
                    .get("quest_id")
                    .expect("item carries quest_id")
                    .clone(),
                entry.get("description").cloned().unwrap_or(Value::Null),
                entry
                    .get("group_type")
                    .cloned()
                    .unwrap_or_else(|| json!(PLAYLIST_GROUP_IMMEDIATE)),
            ),
            other => (other.clone(), Value::Null, json!(PLAYLIST_GROUP_IMMEDIATE)),
        };
        let valid_group = group_type
            .as_str()
            .is_some_and(|g| g == PLAYLIST_GROUP_IMMEDIATE || g == PLAYLIST_GROUP_LONG_HORIZON);
        if !valid_group {
            return Err(QuestError::Invalid(format!(
                "Invalid playlist group type: {}",
                python_str(&group_type)
            )));
        }
        let query = sqlx::query(
            "INSERT INTO quest_playlist_items \
             (playlist_id, quest_id, sort_order, description, group_type) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(playlist_id);
        let query = bind_json(query, &quest_id);
        let query = query.bind(index as i64);
        let query = bind_json(query, &description);
        let query = bind_json(query, &group_type);
        query.execute(&mut *conn).await?;
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
fn normalize_playlist_items(data: &Value) -> Result<Vec<Value>, QuestError> {
    if let Some(items) = data.get("items").filter(|value| !value.is_null()) {
        return Ok(items
            .as_array()
            .expect("items is a list")
            .iter()
            .map(|item| {
                json!({
                    "quest_id": item.get("quest_id").expect("item carries quest_id"),
                    "description": item.get("description").cloned().unwrap_or(Value::Null),
                    "group_type": item
                        .get("group_type")
                        .cloned()
                        .unwrap_or_else(|| json!(PLAYLIST_GROUP_IMMEDIATE)),
                })
            })
            .collect());
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
        .map(|quest_id| {
            json!({
                "quest_id": quest_id,
                "description": null,
                "group_type": PLAYLIST_GROUP_IMMEDIATE,
            })
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
