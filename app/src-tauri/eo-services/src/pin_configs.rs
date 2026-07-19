//! The pin-configuration domain service: CRUD over `pin_configs`.
//!
//! A pin configuration is a *type* of pin scoped to one `(planet, map view)`
//! preset: a palette entry the cartography overlay drops instances of. Placed
//! pins reference their configuration, so colour and special behaviour derive
//! from it (edit the config, restyle the pins) and deleting a config cascades
//! to its placed pins. `category` is `generic` (no behaviour) or `special`;
//! the only special `kind` so far is `tree`, which carries a distinct
//! on-cooldown colour. Cross-field validity (a generic config has no special
//! kind or cooldown colour; a special one requires both) is the caller's
//! concern, matching the `map_pins` persistence-surface split.

use std::sync::Arc;

use crate::clock::Clock;
use crate::db::{Db, DbError};
use crate::time::naive_to_epoch;

#[derive(Debug, thiserror::Error)]
pub enum PinConfigsError {
    /// The addressed configuration does not exist.
    #[error("pin configuration {0} not found")]
    NotFound(i64),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// One pin configuration, as read back, with the count of pins placed from it.
#[derive(Debug, Clone, PartialEq)]
pub struct PinConfig {
    pub id: i64,
    pub planet: String,
    pub map_view_id: Option<i64>,
    pub label: String,
    pub category: String,
    pub special_kind: Option<String>,
    pub icon: String,
    pub radius_m: Option<f64>,
    pub colour: String,
    pub cooldown_colour: Option<String>,
    pub ordinal: i64,
    /// Epoch seconds.
    pub created_at: f64,
    /// How many placed pins reference this configuration.
    pub placed_count: i64,
}

/// A new configuration's fields (id, ordinal, and created_at are assigned).
#[derive(Debug, Clone, PartialEq)]
pub struct NewPinConfig {
    pub planet: String,
    pub map_view_id: Option<i64>,
    pub label: String,
    pub category: String,
    pub special_kind: Option<String>,
    pub icon: String,
    pub radius_m: Option<f64>,
    pub colour: String,
    pub cooldown_colour: Option<String>,
}

/// A full edit of a configuration's style fields; scope (planet, map view)
/// and ordering are immutable through this path.
#[derive(Debug, Clone, PartialEq)]
pub struct PinConfigEdit {
    pub label: String,
    pub category: String,
    pub special_kind: Option<String>,
    pub icon: String,
    pub radius_m: Option<f64>,
    pub colour: String,
    pub cooldown_colour: Option<String>,
}

const READ_COLUMNS: &str = "pc.id, pc.planet, pc.map_view_id, pc.label, pc.category, \
     pc.special_kind, pc.icon, pc.radius_m, pc.colour, pc.cooldown_colour, pc.ordinal, \
     pc.created_at, (SELECT COUNT(*) FROM map_pins WHERE pin_config_id = pc.id)";

pub struct PinConfigsService {
    db: Db,
    clock: Arc<dyn Clock>,
}

impl PinConfigsService {
    pub fn new(db: Db, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }

    /// Every configuration for one planet view, in palette order. `None` is
    /// the Default view.
    pub async fn list(
        &self,
        planet: String,
        map_view_id: Option<i64>,
    ) -> Result<Vec<PinConfig>, DbError> {
        self.db
            .with_reader(move |connection| {
                let sql = format!(
                    "SELECT {READ_COLUMNS} FROM pin_configs pc \
                     WHERE pc.planet = ?1 \
                       AND ((?2 IS NULL AND pc.map_view_id IS NULL) OR pc.map_view_id = ?2) \
                     ORDER BY pc.ordinal, pc.created_at, pc.id"
                );
                let mut stmt = connection.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params![planet, map_view_id], read_config)?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .await
    }

    /// One configuration by id.
    pub async fn get(&self, id: i64) -> Result<PinConfig, PinConfigsError> {
        self.db
            .with_reader(move |connection| {
                let sql = format!("SELECT {READ_COLUMNS} FROM pin_configs pc WHERE pc.id = ?1");
                let mut stmt = connection.prepare(&sql)?;
                let mut rows = stmt.query([id])?;
                match rows.next()? {
                    Some(row) => Ok(Some(read_config(row)?)),
                    None => Ok(None),
                }
            })
            .await?
            .ok_or(PinConfigsError::NotFound(id))
    }

    /// Create a configuration, appended to the end of its preset's palette.
    pub async fn create(&self, config: NewPinConfig) -> Result<PinConfig, DbError> {
        let created_at = naive_to_epoch(self.clock.now());
        let id = self
            .db
            .with_writer(move |connection| {
                let next_ordinal: i64 = connection.query_row(
                    "SELECT COALESCE(MAX(ordinal) + 1, 0) FROM pin_configs \
                     WHERE planet = ?1 AND ((?2 IS NULL AND map_view_id IS NULL) OR map_view_id = ?2)",
                    rusqlite::params![config.planet, config.map_view_id],
                    |row| row.get(0),
                )?;
                connection.execute(
                    "INSERT INTO pin_configs \
                        (planet, map_view_id, label, category, special_kind, icon, radius_m, colour, cooldown_colour, ordinal, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        config.planet,
                        config.map_view_id,
                        config.label,
                        config.category,
                        config.special_kind,
                        config.icon,
                        config.radius_m,
                        config.colour,
                        config.cooldown_colour,
                        next_ordinal,
                        created_at,
                    ],
                )?;
                Ok(connection.last_insert_rowid())
            })
            .await?;
        self.get(id).await.map_err(|error| match error {
            PinConfigsError::Db(error) => error,
            PinConfigsError::NotFound(_) => DbError::from(rusqlite::Error::QueryReturnedNoRows),
        })
    }

    /// Replace a configuration's style fields.
    pub async fn update(&self, id: i64, edit: PinConfigEdit) -> Result<PinConfig, PinConfigsError> {
        let changed = self
            .db
            .with_writer(move |connection| {
                Ok(connection.execute(
                    "UPDATE pin_configs SET label = ?2, category = ?3, special_kind = ?4, \
                            icon = ?5, radius_m = ?6, colour = ?7, cooldown_colour = ?8 \
                     WHERE id = ?1",
                    rusqlite::params![
                        id,
                        edit.label,
                        edit.category,
                        edit.special_kind,
                        edit.icon,
                        edit.radius_m,
                        edit.colour,
                        edit.cooldown_colour,
                    ],
                )?)
            })
            .await?;
        if changed == 0 {
            return Err(PinConfigsError::NotFound(id));
        }
        self.get(id).await
    }

    /// Delete a configuration and its placed pins. Foreign keys are disabled
    /// on the connection (the established pragma surface), so the cascade is
    /// explicit and transactional rather than relying on `ON DELETE CASCADE`.
    pub async fn delete(&self, id: i64) -> Result<(), PinConfigsError> {
        let changed = self
            .db
            .with_writer(move |connection| {
                let tx = connection.transaction()?;
                tx.execute("DELETE FROM map_pins WHERE pin_config_id = ?1", [id])?;
                let changed = tx.execute("DELETE FROM pin_configs WHERE id = ?1", [id])?;
                tx.commit()?;
                Ok(changed)
            })
            .await?;
        if changed == 0 {
            return Err(PinConfigsError::NotFound(id));
        }
        Ok(())
    }

    /// Set the palette order for a preset from a full list of its config ids.
    pub async fn reorder(&self, ids: Vec<i64>) -> Result<(), DbError> {
        self.db
            .with_writer(move |connection| {
                let tx = connection.transaction()?;
                for (ordinal, id) in ids.iter().enumerate() {
                    tx.execute(
                        "UPDATE pin_configs SET ordinal = ?2 WHERE id = ?1",
                        rusqlite::params![id, ordinal as i64],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
    }
}

fn read_config(row: &rusqlite::Row<'_>) -> Result<PinConfig, rusqlite::Error> {
    Ok(PinConfig {
        id: row.get(0)?,
        planet: row.get(1)?,
        map_view_id: row.get(2)?,
        label: row.get(3)?,
        category: row.get(4)?,
        special_kind: row.get(5)?,
        icon: row.get(6)?,
        radius_m: row.get(7)?,
        colour: row.get(8)?,
        cooldown_colour: row.get(9)?,
        ordinal: row.get(10)?,
        created_at: row.get(11)?,
        placed_count: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use crate::map_pins::{MapPinsService, NewMapPin};

    async fn fixture() -> (tempfile::TempDir, PinConfigsService, MapPinsService) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("configs.db")).await.unwrap();
        let clock: Arc<dyn Clock> = Arc::new(MockClock::new(None, 1_000.0));
        let configs = PinConfigsService::new(db.clone(), clock.clone());
        let pins = MapPinsService::new(db, clock);
        (dir, configs, pins)
    }

    fn tree_config() -> NewPinConfig {
        NewPinConfig {
            planet: "Arkadia".into(),
            map_view_id: None,
            label: "Tree".into(),
            category: "special".into(),
            special_kind: Some("tree".into()),
            icon: "🌳".into(),
            radius_m: None,
            colour: "#22c55e".into(),
            cooldown_colour: Some("#f59e0b".into()),
        }
    }

    #[tokio::test]
    async fn configs_order_by_ordinal_and_count_their_placed_pins() {
        let (_dir, configs, pins) = fixture().await;
        let tree = configs.create(tree_config()).await.unwrap();
        let vendor = configs
            .create(NewPinConfig {
                label: "Vendor".into(),
                category: "generic".into(),
                special_kind: None,
                icon: "🏪".into(),
                colour: "#38bdf8".into(),
                cooldown_colour: None,
                ..tree_config()
            })
            .await
            .unwrap();
        assert_eq!(tree.ordinal, 0);
        assert_eq!(vendor.ordinal, 1);

        for _ in 0..3 {
            pins.create(NewMapPin {
                planet: "Arkadia".into(),
                lon: 10.0,
                lat: 20.0,
                altitude: None,
                name: "Tree".into(),
                icon: "🌳".into(),
                kind: "tree".into(),
                radius_m: None,
                notes: None,
                session_id: None,
                map_view_id: None,
                pin_config_id: Some(tree.id),
            })
            .await
            .unwrap();
        }

        let listed = configs.list("Arkadia".into(), None).await.unwrap();
        assert_eq!(
            listed.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![tree.id, vendor.id]
        );
        assert_eq!(listed[0].placed_count, 3);
        assert_eq!(listed[1].placed_count, 0);
    }

    #[tokio::test]
    async fn deleting_a_config_cascades_to_its_placed_pins() {
        let (_dir, configs, pins) = fixture().await;
        let tree = configs.create(tree_config()).await.unwrap();
        let pin = pins
            .create(NewMapPin {
                planet: "Arkadia".into(),
                lon: 10.0,
                lat: 20.0,
                altitude: None,
                name: "Tree".into(),
                icon: "🌳".into(),
                kind: "tree".into(),
                radius_m: None,
                notes: None,
                session_id: None,
                map_view_id: None,
                pin_config_id: Some(tree.id),
            })
            .await
            .unwrap();
        assert_eq!(pin.colour.as_deref(), Some("#22c55e"));
        assert_eq!(pin.special_kind.as_deref(), Some("tree"));

        configs.delete(tree.id).await.unwrap();
        assert!(matches!(
            pins.get(pin.id).await,
            Err(crate::map_pins::MapPinsError::NotFound(_))
        ));
    }
}
