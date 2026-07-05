//! The curated session-link surface: the suggestion tree, accept /
//! decline persistence, and the playlist-matching subset tests.

use std::collections::HashSet;

use serde_json::{json, Value};
use sqlx::Row;

use crate::tracker::naive_to_epoch;

use super::{QuestError, QuestService};

impl QuestService {
    // ── Session link suggestions ────────────────────────────────────

    /// Suggest a curated analytics link for a completed session.
    pub async fn get_session_link_suggestion(&self, session_id: &str) -> Result<Value, QuestError> {
        if let Some((link_type, quest_id, playlist_id)) =
            self.session_analytics_link(session_id).await?
        {
            let reason = if link_type == "declined" {
                "declined"
            } else {
                "already_linked"
            };
            return Ok(json!({
                "suggestion_type": "none",
                "reason": reason,
                "quest_id": quest_id,
                "quest_name": self.quest_name(quest_id).await?,
                "playlist_id": playlist_id,
                "playlist_name": self.playlist_name(playlist_id).await?,
            }));
        }

        let quest_ids = self.session_completed_quest_ids(session_id).await?;
        if quest_ids.is_empty() {
            return Ok(json!({
                "suggestion_type": "none",
                "reason": "no_completions",
                "quest_id": null,
                "quest_name": null,
                "playlist_id": null,
                "playlist_name": null,
            }));
        }

        if quest_ids.len() == 1 {
            let quest_id = quest_ids[0];
            return Ok(json!({
                "suggestion_type": "quest",
                "reason": "single_quest",
                "quest_id": quest_id,
                "quest_name": self.quest_name(Some(quest_id)).await?,
                "playlist_id": null,
                "playlist_name": null,
            }));
        }

        let playlist_ids = self.find_matching_playlists(&quest_ids).await?;
        if playlist_ids.len() == 1 {
            let playlist_id = playlist_ids[0];
            return Ok(json!({
                "suggestion_type": "playlist",
                "reason": "exact_playlist",
                "quest_id": null,
                "quest_name": null,
                "playlist_id": playlist_id,
                "playlist_name": self.playlist_name(Some(playlist_id)).await?,
            }));
        }

        let reason = if playlist_ids.is_empty() {
            "unclean"
        } else {
            "ambiguous_playlist"
        };
        Ok(json!({
            "suggestion_type": "none",
            "reason": reason,
            "quest_id": null,
            "quest_name": null,
            "playlist_id": null,
            "playlist_name": null,
        }))
    }

    /// Persist the current curated analytics suggestion for a session.
    pub async fn accept_session_link_suggestion(
        &self,
        session_id: &str,
    ) -> Result<Value, QuestError> {
        let suggestion = self.get_session_link_suggestion(session_id).await?;
        match suggestion["suggestion_type"].as_str() {
            Some("quest") => {
                self.set_session_analytics_link(
                    session_id,
                    "quest",
                    suggestion["quest_id"].as_i64(),
                    None,
                )
                .await?;
            }
            Some("playlist") => {
                self.set_session_analytics_link(
                    session_id,
                    "playlist",
                    None,
                    suggestion["playlist_id"].as_i64(),
                )
                .await?;
            }
            _ => {
                return Err(QuestError::Invalid(format!(
                    "No linkable suggestion for session {session_id}: {}",
                    suggestion["reason"].as_str().unwrap_or("")
                )));
            }
        }
        Ok(suggestion)
    }

    /// Persist that the user declined curated analytics linkage.
    pub async fn decline_session_link(&self, session_id: &str) -> Result<(), QuestError> {
        self.set_session_analytics_link(session_id, "declined", None, None)
            .await
    }

    async fn session_completed_quest_ids(&self, session_id: &str) -> Result<Vec<i64>, QuestError> {
        let rows = sqlx::query(
            "SELECT DISTINCT quest_id \
             FROM session_quest_completions \
             WHERE session_id = ? \
             ORDER BY quest_id",
        )
        .bind(session_id)
        .fetch_all(self.db.read())
        .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    async fn session_analytics_link(
        &self,
        session_id: &str,
    ) -> Result<Option<(String, Option<i64>, Option<i64>)>, QuestError> {
        Ok(sqlx::query(
            "SELECT session_id, link_type, quest_id, playlist_id \
             FROM session_quest_analytics_links \
             WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(self.db.read())
        .await?
        .map(|row| (row.get(1), row.get(2), row.get(3))))
    }

    async fn set_session_analytics_link(
        &self,
        session_id: &str,
        link_type: &str,
        quest_id: Option<i64>,
        playlist_id: Option<i64>,
    ) -> Result<(), QuestError> {
        sqlx::query(
            "INSERT INTO session_quest_analytics_links \
             (session_id, link_type, quest_id, playlist_id, linked_at) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(session_id) DO UPDATE SET \
                 link_type = excluded.link_type, \
                 quest_id = excluded.quest_id, \
                 playlist_id = excluded.playlist_id, \
                 linked_at = excluded.linked_at",
        )
        .bind(session_id)
        .bind(link_type)
        .bind(quest_id)
        .bind(playlist_id)
        .bind(naive_to_epoch(self.clock.now()))
        .execute(self.db.write())
        .await?;
        Ok(())
    }

    /// Playlists whose immediate set is fully completed while every
    /// completion stays within the playlist's scope.
    async fn find_matching_playlists(
        &self,
        completed_quest_ids: &[i64],
    ) -> Result<Vec<i64>, QuestError> {
        let completed: HashSet<i64> = completed_quest_ids.iter().copied().collect();
        let mut matches = Vec::new();
        for playlist in self.get_playlists(true).await? {
            let ids = |key: &str| -> HashSet<i64> {
                playlist[key]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Value::as_i64)
                    .collect()
            };
            let immediate = ids("immediate_quest_ids");
            if immediate.is_empty() {
                continue;
            }
            let mut scope = immediate.clone();
            scope.extend(ids("long_horizon_quest_ids"));
            if immediate.is_subset(&completed) && completed.is_subset(&scope) {
                matches.push(playlist["id"].as_i64().expect("playlist id"));
            }
        }
        Ok(matches)
    }

    async fn quest_name(&self, quest_id: Option<i64>) -> Result<Option<String>, QuestError> {
        let Some(quest_id) = quest_id else {
            return Ok(None);
        };
        Ok(sqlx::query("SELECT name FROM quests WHERE id = ?")
            .bind(quest_id)
            .fetch_optional(self.db.read())
            .await?
            .map(|row| row.get(0)))
    }

    async fn playlist_name(&self, playlist_id: Option<i64>) -> Result<Option<String>, QuestError> {
        let Some(playlist_id) = playlist_id else {
            return Ok(None);
        };
        Ok(sqlx::query("SELECT name FROM quest_playlists WHERE id = ?")
            .bind(playlist_id)
            .fetch_optional(self.db.read())
            .await?
            .map(|row| row.get(0)))
    }
}
