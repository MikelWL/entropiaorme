//! The curated session-link surface: the suggestion tree, accept /
//! decline persistence, and the playlist-matching subset tests.
//!
//! The suggestion resolves to a typed [`LinkSuggestion`] first and is
//! shaped to its wire form in one place, so the accept flow matches on
//! the resolution rather than re-parsing its own rendered output.

use std::collections::HashSet;

use rusqlite::OptionalExtension as _;
use serde_json::{json, Value};

use super::{QuestError, QuestService};

/// A curated analytics link's kind: the closed vocabulary the link
/// table stores, parsed at the database boundary and rendered back at
/// the bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinkType {
    Quest,
    Playlist,
    Declined,
}

impl LinkType {
    fn as_str(self) -> &'static str {
        match self {
            LinkType::Quest => "quest",
            LinkType::Playlist => "playlist",
            LinkType::Declined => "declined",
        }
    }

    /// Parse a stored `link_type`. Only the three variants are ever
    /// written; an unrecognised value reads as `None`, and the caller
    /// treats the row as a standing (non-declined) link, exactly as
    /// the original's declined-or-not test does.
    fn from_db(raw: &str) -> Option<LinkType> {
        match raw {
            "quest" => Some(LinkType::Quest),
            "playlist" => Some(LinkType::Playlist),
            "declined" => Some(LinkType::Declined),
            _ => None,
        }
    }
}

/// The resolved link suggestion for a session, before wire shaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinkSuggestion {
    /// A link row already exists (a standing link or a decline); the
    /// suggestion is "none" with the row's ids echoed back.
    Linked {
        declined: bool,
        quest_id: Option<i64>,
        playlist_id: Option<i64>,
    },
    /// The session completed nothing.
    NoCompletions,
    /// Exactly one quest completed: suggest linking it.
    SingleQuest(i64),
    /// Several quests completed, exactly one playlist matches: suggest
    /// linking the playlist.
    ExactPlaylist(i64),
    /// Several quests completed, none of the playlists match.
    Unclean,
    /// Several quests completed, more than one playlist matches.
    AmbiguousPlaylist,
}

impl LinkSuggestion {
    /// The wire `reason` naming why this suggestion (or refusal) holds.
    fn reason(self) -> &'static str {
        match self {
            LinkSuggestion::Linked { declined: true, .. } => "declined",
            LinkSuggestion::Linked {
                declined: false, ..
            } => "already_linked",
            LinkSuggestion::NoCompletions => "no_completions",
            LinkSuggestion::SingleQuest(_) => "single_quest",
            LinkSuggestion::ExactPlaylist(_) => "exact_playlist",
            LinkSuggestion::Unclean => "unclean",
            LinkSuggestion::AmbiguousPlaylist => "ambiguous_playlist",
        }
    }
}

impl QuestService {
    // ── Session link suggestions ────────────────────────────────────

    /// Resolve the curated link suggestion for a completed session:
    /// an existing row wins, then the completion count picks the leg
    /// (none / single quest / exact playlist / unclean / ambiguous).
    async fn resolve_link_suggestion(
        &self,
        session_id: &str,
    ) -> Result<LinkSuggestion, QuestError> {
        if let Some((link_type, quest_id, playlist_id)) =
            self.session_analytics_link(session_id).await?
        {
            return Ok(LinkSuggestion::Linked {
                declined: link_type == Some(LinkType::Declined),
                quest_id,
                playlist_id,
            });
        }

        let quest_ids = self.session_completed_quest_ids(session_id).await?;
        if quest_ids.is_empty() {
            return Ok(LinkSuggestion::NoCompletions);
        }
        if quest_ids.len() == 1 {
            return Ok(LinkSuggestion::SingleQuest(quest_ids[0]));
        }

        let playlist_ids = self.find_matching_playlists(&quest_ids).await?;
        Ok(match playlist_ids.as_slice() {
            [playlist_id] => LinkSuggestion::ExactPlaylist(*playlist_id),
            [] => LinkSuggestion::Unclean,
            _ => LinkSuggestion::AmbiguousPlaylist,
        })
    }

    /// Shape a resolved suggestion to its wire form (names looked up
    /// only where the original did).
    async fn shape_link_suggestion(&self, suggestion: LinkSuggestion) -> Result<Value, QuestError> {
        let reason = suggestion.reason();
        Ok(match suggestion {
            LinkSuggestion::Linked {
                quest_id,
                playlist_id,
                ..
            } => json!({
                "suggestion_type": "none",
                "reason": reason,
                "quest_id": quest_id,
                "quest_name": self.quest_name(quest_id).await?,
                "playlist_id": playlist_id,
                "playlist_name": self.playlist_name(playlist_id).await?,
            }),
            LinkSuggestion::SingleQuest(quest_id) => json!({
                "suggestion_type": "quest",
                "reason": reason,
                "quest_id": quest_id,
                "quest_name": self.quest_name(Some(quest_id)).await?,
                "playlist_id": null,
                "playlist_name": null,
            }),
            LinkSuggestion::ExactPlaylist(playlist_id) => json!({
                "suggestion_type": "playlist",
                "reason": reason,
                "quest_id": null,
                "quest_name": null,
                "playlist_id": playlist_id,
                "playlist_name": self.playlist_name(Some(playlist_id)).await?,
            }),
            LinkSuggestion::NoCompletions
            | LinkSuggestion::Unclean
            | LinkSuggestion::AmbiguousPlaylist => json!({
                "suggestion_type": "none",
                "reason": reason,
                "quest_id": null,
                "quest_name": null,
                "playlist_id": null,
                "playlist_name": null,
            }),
        })
    }

    /// Suggest a curated analytics link for a completed session.
    pub async fn get_session_link_suggestion(&self, session_id: &str) -> Result<Value, QuestError> {
        let suggestion = self.resolve_link_suggestion(session_id).await?;
        self.shape_link_suggestion(suggestion).await
    }

    /// Persist the current curated analytics suggestion for a session.
    pub async fn accept_session_link_suggestion(
        &self,
        session_id: &str,
    ) -> Result<Value, QuestError> {
        let suggestion = self.resolve_link_suggestion(session_id).await?;
        let shaped = self.shape_link_suggestion(suggestion).await?;
        match suggestion {
            LinkSuggestion::SingleQuest(quest_id) => {
                self.set_session_analytics_link(session_id, LinkType::Quest, Some(quest_id), None)
                    .await?;
            }
            LinkSuggestion::ExactPlaylist(playlist_id) => {
                self.set_session_analytics_link(
                    session_id,
                    LinkType::Playlist,
                    None,
                    Some(playlist_id),
                )
                .await?;
            }
            _ => {
                return Err(QuestError::Invalid(format!(
                    "No linkable suggestion for session {session_id}: {}",
                    suggestion.reason()
                )));
            }
        }
        Ok(shaped)
    }

    /// Persist that the user declined curated analytics linkage.
    pub async fn decline_session_link(&self, session_id: &str) -> Result<(), QuestError> {
        self.set_session_analytics_link(session_id, LinkType::Declined, None, None)
            .await
    }

    async fn session_completed_quest_ids(&self, session_id: &str) -> Result<Vec<i64>, QuestError> {
        let session_id = session_id.to_string();
        Ok(self
            .db
            .with_reader(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT quest_id \
                     FROM session_quest_completions \
                     WHERE session_id = ? \
                     ORDER BY quest_id",
                )?;
                let mut rows = stmt.query(rusqlite::params![session_id])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push(row.get::<_, i64>(0)?);
                }
                Ok(out)
            })
            .await?)
    }

    async fn session_analytics_link(
        &self,
        session_id: &str,
    ) -> Result<Option<(Option<LinkType>, Option<i64>, Option<i64>)>, QuestError> {
        let session_id = session_id.to_string();
        Ok(self
            .db
            .with_reader(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT session_id, link_type, quest_id, playlist_id \
                         FROM session_quest_analytics_links \
                         WHERE session_id = ?",
                        rusqlite::params![session_id],
                        |row| {
                            Ok((
                                LinkType::from_db(row.get::<_, String>(1)?.as_str()),
                                row.get::<_, Option<i64>>(2)?,
                                row.get::<_, Option<i64>>(3)?,
                            ))
                        },
                    )
                    .optional()?)
            })
            .await?)
    }

    async fn set_session_analytics_link(
        &self,
        session_id: &str,
        link_type: LinkType,
        quest_id: Option<i64>,
        playlist_id: Option<i64>,
    ) -> Result<(), QuestError> {
        let session_id = session_id.to_string();
        let link_type = link_type.as_str();
        let now = self.now_epoch();
        self.db
            .with_writer(move |conn| {
                conn.execute(
                    "INSERT INTO session_quest_analytics_links \
                     (session_id, link_type, quest_id, playlist_id, linked_at) \
                     VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT(session_id) DO UPDATE SET \
                         link_type = excluded.link_type, \
                         quest_id = excluded.quest_id, \
                         playlist_id = excluded.playlist_id, \
                         linked_at = excluded.linked_at",
                    rusqlite::params![session_id, link_type, quest_id, playlist_id, now],
                )?;
                Ok(())
            })
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
        Ok(self
            .db
            .with_reader(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT name FROM quests WHERE id = ?",
                        rusqlite::params![quest_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?)
            })
            .await?)
    }

    async fn playlist_name(&self, playlist_id: Option<i64>) -> Result<Option<String>, QuestError> {
        let Some(playlist_id) = playlist_id else {
            return Ok(None);
        };
        Ok(self
            .db
            .with_reader(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT name FROM quest_playlists WHERE id = ?",
                        rusqlite::params![playlist_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?)
            })
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_type_parses_each_stored_variant_and_rejects_the_rest() {
        assert_eq!(LinkType::from_db("quest"), Some(LinkType::Quest));
        assert_eq!(LinkType::from_db("playlist"), Some(LinkType::Playlist));
        assert_eq!(LinkType::from_db("declined"), Some(LinkType::Declined));
        // An unknown value reads as a standing (non-declined) link.
        assert_eq!(LinkType::from_db("something-else"), None);
        assert_eq!(LinkType::from_db(""), None);
    }
}
