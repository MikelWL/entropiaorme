//! The cartography-pin domain service: CRUD over the `map_pins` table.
//!
//! Pins are durable user data: a named, icon-carrying location on one
//! of the bundled planet maps, either an exact point or an area of a
//! given radius in metres, optionally backlinked to the tracked session
//! it was dropped during. Coordinates are game units on the global tile
//! grid; plausibility gating against a planet's calibrated bounds is
//! the caller's concern (the facade owns the map catalogue), so this
//! service stays a pure persistence surface. `kind` and `icon` are
//! user-shaped presentation vocabulary (the pin palette is
//! user-configured), deliberately open strings rather than a closed
//! set.

use std::sync::Arc;

use rusqlite::OptionalExtension;

use crate::clock::Clock;
use crate::db::{Db, DbError};
use crate::navigation::COOLDOWN_SECONDS;
use crate::time::naive_to_epoch;

/// The pin domain service over the shared database and injected clock.
pub struct MapPinsService {
    db: Db,
    clock: Arc<dyn Clock>,
}

#[derive(Debug, thiserror::Error)]
pub enum MapPinsError {
    /// The addressed pin does not exist.
    #[error("map pin {0} not found")]
    NotFound(i64),
    /// The addressed named map view does not exist.
    #[error("map view {0} not found")]
    ViewNotFound(i64),
    /// Names are unique within one planet, ignoring case.
    #[error("a map view named {0:?} already exists on {1}")]
    ViewNameTaken(String, String),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// One named pin-set view over a planet's bundled raster.
#[derive(Debug, Clone, PartialEq)]
pub struct MapView {
    pub id: i64,
    pub planet: String,
    pub name: String,
    /// Epoch seconds.
    pub created_at: f64,
}

/// One stored pin, as read back.
#[derive(Debug, Clone, PartialEq)]
pub struct MapPin {
    pub id: i64,
    pub planet: String,
    pub lon: f64,
    pub lat: f64,
    pub altitude: Option<f64>,
    pub name: String,
    pub icon: String,
    pub kind: String,
    pub radius_m: Option<f64>,
    pub notes: Option<String>,
    pub session_id: Option<String>,
    pub map_view_id: Option<i64>,
    /// Epoch seconds.
    pub created_at: f64,
    /// Epoch seconds of the most recent confirmed visit, if any. A read-only
    /// projection of the separate visit records; the pin itself stays visit
    /// agnostic.
    pub last_visited_at: Option<f64>,
    /// Epoch seconds until which the pin's most recent visit keeps it on
    /// cooldown, if any. Derived from `last_visited_at` and the cooldown
    /// policy so the policy stays owned by one place.
    pub cooldown_until: Option<f64>,
    /// The palette configuration this pin is an instance of, if any. Colour,
    /// category, and special behaviour derive from it (below).
    pub pin_config_id: Option<i64>,
    /// The pin's colour, from its configuration (generic colour or special
    /// active colour). `None` when the pin has no configuration.
    pub colour: Option<String>,
    /// The special-tree on-cooldown colour, from its configuration.
    pub cooldown_colour: Option<String>,
    /// The configuration's category (`generic` / `special`), if any.
    pub category: Option<String>,
    /// The configuration's special kind (`tree`), if any.
    pub special_kind: Option<String>,
}

/// A new pin's fields (id and created_at are service-assigned).
#[derive(Debug, Clone, PartialEq)]
pub struct NewMapPin {
    pub planet: String,
    pub lon: f64,
    pub lat: f64,
    pub altitude: Option<f64>,
    pub name: String,
    pub icon: String,
    pub kind: String,
    pub radius_m: Option<f64>,
    pub notes: Option<String>,
    pub session_id: Option<String>,
    pub map_view_id: Option<i64>,
    pub pin_config_id: Option<i64>,
}

/// A partial update: `None` leaves a field untouched. The nullable
/// columns (altitude, radius_m, notes) use a double-`Option` so "set
/// to null" and "leave alone" stay distinct.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapPinPatch {
    pub lon: Option<f64>,
    pub lat: Option<f64>,
    pub altitude: Option<Option<f64>>,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub kind: Option<String>,
    pub radius_m: Option<Option<f64>>,
    pub notes: Option<Option<String>>,
}

/// The pin read columns and join, shared by every pin query. Aliased `mp`
/// (the pin) and `pc` (its configuration): colour, category, and special kind
/// come from the joined configuration; the latest-visit subquery drives the
/// cooldown projection.
const PIN_COLUMNS: &str = "mp.id, mp.planet, mp.lon, mp.lat, mp.altitude, mp.name, mp.icon, \
     mp.kind, mp.radius_m, mp.notes, mp.session_id, mp.map_view_id, mp.created_at, \
     (SELECT MAX(visited_at) FROM map_pin_visits WHERE pin_id = mp.id), \
     mp.pin_config_id, pc.colour, pc.cooldown_colour, pc.category, pc.special_kind";
const PIN_FROM: &str = "FROM map_pins mp LEFT JOIN pin_configs pc ON pc.id = mp.pin_config_id";

impl MapPinsService {
    pub fn new(db: Db, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }

    /// Every pin in one planet view, newest first. `None` is Default.
    pub async fn list(
        &self,
        planet: String,
        map_view_id: Option<i64>,
    ) -> Result<Vec<MapPin>, DbError> {
        self.db
            .with_reader(move |connection| {
                let sql = format!(
                    "SELECT {PIN_COLUMNS} {PIN_FROM} WHERE mp.planet = ?1 \
                       AND ((?2 IS NULL AND mp.map_view_id IS NULL) OR mp.map_view_id = ?2) \
                     ORDER BY mp.created_at DESC, mp.id DESC"
                );
                let mut stmt = connection.prepare(&sql)?;
                let mut rows = stmt.query(rusqlite::params![planet, map_view_id])?;
                let mut pins = Vec::new();
                while let Some(row) = rows.next()? {
                    pins.push(read_pin(row)?);
                }
                Ok(pins)
            })
            .await
    }

    /// Pins intersecting a coordinate viewport, newest first. The
    /// compound spatial index narrows the read before any rendering or
    /// precise distance work reaches the caller.
    pub async fn list_in_bounds(
        &self,
        planet: String,
        map_view_id: Option<i64>,
        lon_min: f64,
        lon_max: f64,
        lat_min: f64,
        lat_max: f64,
    ) -> Result<Vec<MapPin>, DbError> {
        self.db
            .with_reader(move |connection| {
                let sql = format!(
                    "SELECT {PIN_COLUMNS} {PIN_FROM} WHERE mp.planet = ?1 \
                       AND ((?2 IS NULL AND mp.map_view_id IS NULL) OR mp.map_view_id = ?2) \
                       AND mp.lon BETWEEN ?3 AND ?4 AND mp.lat BETWEEN ?5 AND ?6 \
                     ORDER BY mp.created_at DESC, mp.id DESC"
                );
                let mut stmt = connection.prepare(&sql)?;
                let rows = stmt.query_map(
                    rusqlite::params![planet, map_view_id, lon_min, lon_max, lat_min, lat_max],
                    read_pin,
                )?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .await
    }

    /// The nearest pin inside `radius`, using an index-friendly bounding
    /// square followed by exact Euclidean distance.
    pub async fn nearby(
        &self,
        planet: String,
        map_view_id: Option<i64>,
        lon: f64,
        lat: f64,
        radius: f64,
    ) -> Result<Option<(MapPin, f64)>, DbError> {
        let lon_min = lon - radius;
        let lon_max = lon + radius;
        let lat_min = lat - radius;
        let lat_max = lat + radius;
        let candidates = self
            .list_in_bounds(planet, map_view_id, lon_min, lon_max, lat_min, lat_max)
            .await?;
        Ok(candidates
            .into_iter()
            .filter_map(|pin| {
                let distance = (pin.lon - lon).hypot(pin.lat - lat);
                (distance <= radius).then_some((pin, distance))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1)))
    }

    /// One pin by id.
    pub async fn get(&self, id: i64) -> Result<MapPin, MapPinsError> {
        self.db
            .with_reader(move |connection| {
                let sql = format!("SELECT {PIN_COLUMNS} {PIN_FROM} WHERE mp.id = ?1");
                let mut stmt = connection.prepare(&sql)?;
                let mut rows = stmt.query([id])?;
                match rows.next()? {
                    Some(row) => Ok(Some(read_pin(row)?)),
                    None => Ok(None),
                }
            })
            .await?
            .ok_or(MapPinsError::NotFound(id))
    }

    /// Create a pin; returns it as stored (with its configuration's colour and
    /// category joined in).
    pub async fn create(&self, pin: NewMapPin) -> Result<MapPin, DbError> {
        let created_at = naive_to_epoch(self.clock.now());
        let id = self
            .db
            .with_writer(move |connection| {
                connection.execute(
                    "INSERT INTO map_pins (planet, lon, lat, altitude, name, icon, \
                                           kind, radius_m, notes, session_id, map_view_id, pin_config_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    rusqlite::params![
                        pin.planet,
                        pin.lon,
                        pin.lat,
                        pin.altitude,
                        pin.name,
                        pin.icon,
                        pin.kind,
                        pin.radius_m,
                        pin.notes,
                        pin.session_id,
                        pin.map_view_id,
                        pin.pin_config_id,
                        created_at,
                    ],
                )?;
                Ok(connection.last_insert_rowid())
            })
            .await?;
        self.get(id).await.map_err(|error| match error {
            MapPinsError::Db(error) => error,
            _ => DbError::from(rusqlite::Error::QueryReturnedNoRows),
        })
    }

    /// Apply a partial update; returns the pin as stored afterwards.
    pub async fn update(&self, id: i64, patch: MapPinPatch) -> Result<MapPin, MapPinsError> {
        self.db
            .with_writer(move |connection| {
                let existing = {
                    let sql = format!("SELECT {PIN_COLUMNS} {PIN_FROM} WHERE mp.id = ?1");
                    let mut stmt = connection.prepare(&sql)?;
                    let mut rows = stmt.query([id])?;
                    match rows.next()? {
                        Some(row) => read_pin(row)?,
                        None => return Ok(None),
                    }
                };
                let updated = MapPin {
                    lon: patch.lon.unwrap_or(existing.lon),
                    lat: patch.lat.unwrap_or(existing.lat),
                    altitude: patch.altitude.unwrap_or(existing.altitude),
                    name: patch.name.unwrap_or(existing.name),
                    icon: patch.icon.unwrap_or(existing.icon),
                    kind: patch.kind.unwrap_or(existing.kind),
                    radius_m: patch.radius_m.unwrap_or(existing.radius_m),
                    notes: patch.notes.unwrap_or(existing.notes),
                    ..existing
                };
                connection.execute(
                    "UPDATE map_pins SET lon = ?2, lat = ?3, altitude = ?4, name = ?5, \
                            icon = ?6, kind = ?7, radius_m = ?8, notes = ?9 \
                     WHERE id = ?1",
                    rusqlite::params![
                        id,
                        updated.lon,
                        updated.lat,
                        updated.altitude,
                        updated.name,
                        updated.icon,
                        updated.kind,
                        updated.radius_m,
                        updated.notes,
                    ],
                )?;
                Ok(Some(updated))
            })
            .await?
            .ok_or(MapPinsError::NotFound(id))
    }

    /// Delete a pin.
    pub async fn delete(&self, id: i64) -> Result<(), MapPinsError> {
        let changed = self
            .db
            .with_writer(move |connection| {
                Ok(connection.execute("DELETE FROM map_pins WHERE id = ?1", [id])?)
            })
            .await?;
        if changed == 0 {
            return Err(MapPinsError::NotFound(id));
        }
        Ok(())
    }

    /// Named views on a planet, oldest first. Default is virtual and is
    /// therefore not returned here.
    pub async fn list_views(&self, planet: String) -> Result<Vec<MapView>, DbError> {
        self.db
            .with_reader(move |connection| {
                let mut stmt = connection.prepare(
                    "SELECT id, planet, name, created_at FROM map_views \
                     WHERE planet = ?1 ORDER BY created_at ASC, id ASC",
                )?;
                let rows = stmt.query_map([planet], |row| {
                    Ok(MapView {
                        id: row.get(0)?,
                        planet: row.get(1)?,
                        name: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                })?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .await
    }

    /// One named view by id.
    pub async fn get_view(&self, id: i64) -> Result<MapView, MapPinsError> {
        self.db
            .with_reader(move |connection| {
                Ok(connection
                    .query_row(
                        "SELECT id, planet, name, created_at FROM map_views WHERE id = ?1",
                        [id],
                        |row| {
                            Ok(MapView {
                                id: row.get(0)?,
                                planet: row.get(1)?,
                                name: row.get(2)?,
                                created_at: row.get(3)?,
                            })
                        },
                    )
                    .optional()?)
            })
            .await?
            .ok_or(MapPinsError::ViewNotFound(id))
    }

    /// Create a named view. Names are unique per planet, ignoring case.
    pub async fn create_view(&self, planet: String, name: String) -> Result<MapView, MapPinsError> {
        let created_at = naive_to_epoch(self.clock.now());
        let duplicate_name = name.clone();
        let duplicate_planet = planet.clone();
        self.db
            .with_writer(move |connection| {
                let exists: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM map_views WHERE planet = ?1 AND name = ?2 COLLATE NOCASE)",
                    rusqlite::params![planet, name],
                    |row| row.get(0),
                )?;
                if exists {
                    return Ok(None);
                }
                connection.execute(
                    "INSERT INTO map_views (planet, name, created_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![planet, name, created_at],
                )?;
                Ok(Some(MapView {
                    id: connection.last_insert_rowid(),
                    planet,
                    name,
                    created_at,
                }))
            })
            .await?
            .ok_or(MapPinsError::ViewNameTaken(
                duplicate_name,
                duplicate_planet,
            ))
    }

    /// Rename a named view.
    pub async fn rename_view(&self, id: i64, name: String) -> Result<MapView, MapPinsError> {
        self.db
            .with_writer(move |connection| {
                let existing = connection
                    .query_row(
                        "SELECT id, planet, name, created_at FROM map_views WHERE id = ?1",
                        [id],
                        |row| {
                            Ok(MapView {
                                id: row.get(0)?,
                                planet: row.get(1)?,
                                name: row.get(2)?,
                                created_at: row.get(3)?,
                            })
                        },
                    )
                    .optional()?;
                let Some(mut view) = existing else {
                    return Ok(Err(MapPinsError::ViewNotFound(id)));
                };
                let exists: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM map_views WHERE planet = ?1 AND name = ?2 COLLATE NOCASE AND id != ?3)",
                    rusqlite::params![view.planet, name, id],
                    |row| row.get(0),
                )?;
                if exists {
                    return Ok(Err(MapPinsError::ViewNameTaken(name, view.planet)));
                }
                connection.execute("UPDATE map_views SET name = ?2 WHERE id = ?1", rusqlite::params![id, name])?;
                view.name = name;
                Ok(Ok(view))
            })
            .await?
    }

    /// Delete a named view and all its pins in one writer transaction.
    ///
    /// The database deliberately runs with foreign-key enforcement off,
    /// so the service owns the cascade rather than relying on the
    /// declarative reference in the schema.
    pub async fn delete_view(&self, id: i64) -> Result<(), MapPinsError> {
        let changed = self
            .db
            .with_writer(move |connection| {
                let transaction = connection.transaction()?;
                let exists: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM map_views WHERE id = ?1)",
                    [id],
                    |row| row.get(0),
                )?;
                if !exists {
                    transaction.commit()?;
                    return Ok(0);
                }
                transaction.execute("DELETE FROM map_pins WHERE map_view_id = ?1", [id])?;
                transaction.execute("DELETE FROM pin_configs WHERE map_view_id = ?1", [id])?;
                let changed = transaction.execute("DELETE FROM map_views WHERE id = ?1", [id])?;
                transaction.commit()?;
                Ok(changed)
            })
            .await?;
        if changed == 0 {
            return Err(MapPinsError::ViewNotFound(id));
        }
        Ok(())
    }
}

fn read_pin(row: &rusqlite::Row<'_>) -> Result<MapPin, rusqlite::Error> {
    let last_visited_at: Option<f64> = row.get(13)?;
    Ok(MapPin {
        id: row.get(0)?,
        planet: row.get(1)?,
        lon: row.get(2)?,
        lat: row.get(3)?,
        altitude: row.get(4)?,
        name: row.get(5)?,
        icon: row.get(6)?,
        kind: row.get(7)?,
        radius_m: row.get(8)?,
        notes: row.get(9)?,
        session_id: row.get(10)?,
        map_view_id: row.get(11)?,
        created_at: row.get(12)?,
        last_visited_at,
        cooldown_until: last_visited_at.map(|visited| visited + COOLDOWN_SECONDS),
        pin_config_id: row.get(14)?,
        colour: row.get(15)?,
        cooldown_colour: row.get(16)?,
        category: row.get(17)?,
        special_kind: row.get(18)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    fn new_pin() -> NewMapPin {
        NewMapPin {
            planet: "Calypso".into(),
            lon: 10.0,
            lat: 20.0,
            altitude: None,
            name: "Tree".into(),
            icon: "🌲".into(),
            kind: "tree".into(),
            radius_m: None,
            notes: None,
            session_id: None,
            map_view_id: None,
            pin_config_id: None,
        }
    }

    #[tokio::test]
    async fn a_pin_read_projects_its_latest_visit_and_cooldown() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pins.db")).await.unwrap();
        let clock: Arc<dyn Clock> = Arc::new(MockClock::new(None, 1_000.0));
        let service = MapPinsService::new(db.clone(), clock);

        let pin = service.create(new_pin()).await.unwrap();
        assert_eq!(pin.last_visited_at, None);
        assert_eq!(pin.cooldown_until, None);

        let pin_id = pin.id;
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO map_pin_visits (pin_id, run_id, visited_at, source, outcome, observed_lon, observed_lat, observed_distance) VALUES (?1, NULL, 5000.0, 'manual', 'manual', 10.0, 20.0, 0.0)",
                [pin_id],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let read = service.get(pin_id).await.unwrap();
        assert_eq!(read.last_visited_at, Some(5_000.0));
        assert_eq!(read.cooldown_until, Some(5_000.0 + COOLDOWN_SECONDS));
    }
}
