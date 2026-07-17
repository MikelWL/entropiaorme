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

use crate::clock::Clock;
use crate::db::{Db, DbError};
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
    #[error(transparent)]
    Db(#[from] DbError),
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
    /// Epoch seconds.
    pub created_at: f64,
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

impl MapPinsService {
    pub fn new(db: Db, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }

    /// Every pin on a planet, newest first.
    pub async fn list(&self, planet: String) -> Result<Vec<MapPin>, DbError> {
        self.db
            .with_reader(move |connection| {
                let mut stmt = connection.prepare(
                    "SELECT id, planet, lon, lat, altitude, name, icon, kind, \
                            radius_m, notes, session_id, created_at \
                     FROM map_pins WHERE planet = ?1 \
                     ORDER BY created_at DESC, id DESC",
                )?;
                let mut rows = stmt.query([&planet])?;
                let mut pins = Vec::new();
                while let Some(row) = rows.next()? {
                    pins.push(read_pin(row)?);
                }
                Ok(pins)
            })
            .await
    }

    /// One pin by id.
    pub async fn get(&self, id: i64) -> Result<MapPin, MapPinsError> {
        self.db
            .with_reader(move |connection| {
                let mut stmt = connection.prepare(
                    "SELECT id, planet, lon, lat, altitude, name, icon, kind, \
                            radius_m, notes, session_id, created_at \
                     FROM map_pins WHERE id = ?1",
                )?;
                let mut rows = stmt.query([id])?;
                match rows.next()? {
                    Some(row) => Ok(Some(read_pin(row)?)),
                    None => Ok(None),
                }
            })
            .await?
            .ok_or(MapPinsError::NotFound(id))
    }

    /// Create a pin; returns it as stored.
    pub async fn create(&self, pin: NewMapPin) -> Result<MapPin, DbError> {
        let created_at = naive_to_epoch(self.clock.now());
        self.db
            .with_writer(move |connection| {
                connection.execute(
                    "INSERT INTO map_pins (planet, lon, lat, altitude, name, icon, \
                                           kind, radius_m, notes, session_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                        created_at,
                    ],
                )?;
                let id = connection.last_insert_rowid();
                Ok(MapPin {
                    id,
                    planet: pin.planet,
                    lon: pin.lon,
                    lat: pin.lat,
                    altitude: pin.altitude,
                    name: pin.name,
                    icon: pin.icon,
                    kind: pin.kind,
                    radius_m: pin.radius_m,
                    notes: pin.notes,
                    session_id: pin.session_id,
                    created_at,
                })
            })
            .await
    }

    /// Apply a partial update; returns the pin as stored afterwards.
    pub async fn update(&self, id: i64, patch: MapPinPatch) -> Result<MapPin, MapPinsError> {
        self.db
            .with_writer(move |connection| {
                let existing = {
                    let mut stmt = connection.prepare(
                        "SELECT id, planet, lon, lat, altitude, name, icon, kind, \
                                radius_m, notes, session_id, created_at \
                         FROM map_pins WHERE id = ?1",
                    )?;
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
}

fn read_pin(row: &rusqlite::Row<'_>) -> Result<MapPin, rusqlite::Error> {
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
        created_at: row.get(11)?,
    })
}
