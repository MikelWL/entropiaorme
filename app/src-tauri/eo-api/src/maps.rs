//! The planet-maps family: the bundled map catalogue read and the raster
//! fetch behind it.
//!
//! The catalogue is a shipped resource (`resources/maps/`): per-planet
//! rasters plus tile-grid calibration, loaded once at composition into a
//! [`PlanetMapStore`]. The typed read serves the full catalogue (the
//! frontend derives its pixel/game-unit transforms from the calibration
//! fields); the raster itself is raw bytes and rides a bespoke shell
//! command outside the manifest, exactly like the manual-scan capture
//! preview. A facade composed without the bundle serves an empty
//! catalogue: the maps surface stands down, nothing errors at startup.

use eo_services::map_pins::{MapPinsError, NewMapPin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::settings::double_option;
use crate::Nullable;
use crate::{Api, ApiError};

/// A map's coordinate window in game units: the plausibility gate for
/// any coordinate claimed to lie on it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanetMapBounds {
    pub lon_min: i64,
    pub lon_max: i64,
    pub lat_min: i64,
    pub lat_max: i64,
}

/// A calibrated map's placement on the global tile grid, with the
/// per-axis pixel scales the frontend renders through.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanetMapCalibration {
    pub tile_origin_x: i64,
    pub tile_origin_y: i64,
    pub tile_width: i64,
    pub tile_height: i64,
    pub units_per_pixel_x: f64,
    pub units_per_pixel_y: f64,
    pub bounds: PlanetMapBounds,
}

/// One planet/instance map in the bundled catalogue. `calibration` is
/// null for a view-only map (displayable, but coordinates cannot be
/// placed on it); `technical_name` is null when the in-game waypoint
/// name is unknown.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanetMap {
    pub name: String,
    pub technical_name: Nullable<String>,
    pub image_mime: String,
    pub image_width_px: u32,
    pub image_height_px: u32,
    pub calibration: Nullable<PlanetMapCalibration>,
}

impl Api {
    /// The bundled planet-map catalogue, in bundle order; empty when the
    /// facade was composed without the map bundle.
    pub fn planet_maps(&self) -> Result<Vec<PlanetMap>, ApiError> {
        let Some(store) = self.planet_maps.as_ref() else {
            return Ok(Vec::new());
        };
        Ok(store.records().iter().map(to_dto).collect())
    }

    /// A planet map's raster bytes (the shell base64-encodes them for a
    /// `data:` URL; the MIME type rides the catalogue read).
    pub fn planet_map_image(&self, planet_name: &str) -> Result<Vec<u8>, ApiError> {
        let store = self
            .planet_maps
            .as_ref()
            .ok_or_else(|| ApiError::invalid_state("planet-map bundle unavailable"))?;
        store
            .image_bytes(planet_name)
            .ok_or_else(|| ApiError::not_found(format!("no map for planet {planet_name}")))
    }
}

// ── Pins ────────────────────────────────────────────────────────────

/// One stored cartography pin. Coordinates are game units; `radius_m`
/// null marks an exact point, a value an area pin of that radius in
/// metres; `session_id` backlinks the tracked session the pin was
/// dropped during, when any.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MapPin {
    pub id: i64,
    pub planet: String,
    pub lon: f64,
    pub lat: f64,
    pub altitude: Nullable<f64>,
    pub name: String,
    pub icon: String,
    pub kind: String,
    pub radius_m: Nullable<f64>,
    pub notes: Nullable<String>,
    pub session_id: Nullable<String>,
    /// Epoch seconds.
    pub created_at: f64,
}

/// A new pin's fields (id and creation time are assigned server-side).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MapPinInput {
    pub planet: String,
    pub lon: f64,
    pub lat: f64,
    #[serde(default)]
    pub altitude: Option<f64>,
    pub name: String,
    pub icon: String,
    pub kind: String,
    #[serde(default)]
    pub radius_m: Option<f64>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// A partial pin update: absent fields stay untouched. The nullable
/// fields (altitude, radius, notes) are double options so an explicit
/// `null` (clear it) stays distinct from an absent field.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MapPinPatch {
    #[serde(default)]
    pub lon: Option<f64>,
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default, deserialize_with = "double_option")]
    pub altitude: Option<Option<f64>>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub radius_m: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
}

impl Api {
    /// Every pin on a planet, newest first.
    pub async fn map_pins_list(&self, planet: String) -> Result<Vec<MapPin>, ApiError> {
        let pins = self.map_pins.list(planet).await.map_err(db_error)?;
        Ok(pins.into_iter().map(pin_to_dto).collect())
    }

    /// Create a pin. When the planet is in the bundled catalogue and
    /// calibrated, the coordinates must lie inside its bounds: an
    /// implausible pin is refused, never silently stored.
    pub async fn map_pin_create(&self, pin: MapPinInput) -> Result<MapPin, ApiError> {
        self.validate_pin_coords(&pin.planet, pin.lon, pin.lat)?;
        if pin.name.trim().is_empty() {
            return Err(ApiError::bad_request("a pin needs a name"));
        }
        if let Some(radius) = pin.radius_m {
            if !(radius > 0.0) {
                return Err(ApiError::bad_request("a pin radius must be positive"));
            }
        }
        let stored = self
            .map_pins
            .create(NewMapPin {
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
            })
            .await
            .map_err(db_error)?;
        Ok(pin_to_dto(stored))
    }

    /// Apply a partial update; a moved pin re-clears the bounds gate.
    pub async fn map_pin_update(&self, id: i64, patch: MapPinPatch) -> Result<MapPin, ApiError> {
        if let Some(name) = patch.name.as_deref() {
            if name.trim().is_empty() {
                return Err(ApiError::bad_request("a pin needs a name"));
            }
        }
        if let Some(Some(radius)) = patch.radius_m {
            if !(radius > 0.0) {
                return Err(ApiError::bad_request("a pin radius must be positive"));
            }
        }
        if patch.lon.is_some() || patch.lat.is_some() {
            // The move gate needs the pin's planet (and its other axis);
            // read it first so a cross-bounds move is refused untouched.
            let current = self.map_pins.get(id).await.map_err(pins_error)?;
            let lon = patch.lon.unwrap_or(current.lon);
            let lat = patch.lat.unwrap_or(current.lat);
            self.validate_pin_coords(&current.planet, lon, lat)?;
        }
        let stored = self
            .map_pins
            .update(
                id,
                eo_services::map_pins::MapPinPatch {
                    lon: patch.lon,
                    lat: patch.lat,
                    altitude: patch.altitude,
                    name: patch.name,
                    icon: patch.icon,
                    kind: patch.kind,
                    radius_m: patch.radius_m,
                    notes: patch.notes,
                },
            )
            .await
            .map_err(pins_error)?;
        Ok(pin_to_dto(stored))
    }

    /// Delete a pin.
    pub async fn map_pin_delete(&self, id: i64) -> Result<(), ApiError> {
        self.map_pins.delete(id).await.map_err(pins_error)
    }

    /// The bounds gate: refuse coordinates outside a calibrated map's
    /// window. An uncatalogued or uncalibrated planet passes (nothing
    /// authoritative to gate against); the facade never invents bounds.
    fn validate_pin_coords(&self, planet: &str, lon: f64, lat: f64) -> Result<(), ApiError> {
        let Some(store) = self.planet_maps.as_ref() else {
            return Ok(());
        };
        let Some(bounds) = store
            .record(planet)
            .and_then(|record| record.calibration.as_ref())
            .map(|cal| cal.bounds)
        else {
            return Ok(());
        };
        if !bounds.contains(lon.round() as i64, lat.round() as i64) {
            return Err(ApiError::bad_request(format!(
                "coordinates ({lon}, {lat}) lie outside {planet}'s map bounds"
            )));
        }
        Ok(())
    }
}

fn pin_to_dto(pin: eo_services::map_pins::MapPin) -> MapPin {
    MapPin {
        id: pin.id,
        planet: pin.planet,
        lon: pin.lon,
        lat: pin.lat,
        altitude: pin.altitude.into(),
        name: pin.name,
        icon: pin.icon,
        kind: pin.kind,
        radius_m: pin.radius_m.into(),
        notes: pin.notes.into(),
        session_id: pin.session_id.into(),
        created_at: pin.created_at,
    }
}

fn db_error(err: eo_services::db::DbError) -> ApiError {
    ApiError::internal("map pins")(err)
}

fn pins_error(err: MapPinsError) -> ApiError {
    match err {
        MapPinsError::NotFound(id) => ApiError::not_found(format!("map pin {id} not found")),
        MapPinsError::Db(err) => ApiError::internal("map pins")(err),
    }
}

fn to_dto(record: &eo_services::planet_maps::PlanetMapRecord) -> PlanetMap {
    PlanetMap {
        name: record.name.clone(),
        technical_name: record.technical_name.clone().into(),
        image_mime: record.image_mime.to_string(),
        image_width_px: record.image_width_px,
        image_height_px: record.image_height_px,
        calibration: record
            .calibration
            .as_ref()
            .map(|cal| PlanetMapCalibration {
                tile_origin_x: cal.tile_origin_x,
                tile_origin_y: cal.tile_origin_y,
                tile_width: cal.tile_width,
                tile_height: cal.tile_height,
                units_per_pixel_x: cal.units_per_pixel_x,
                units_per_pixel_y: cal.units_per_pixel_y,
                bounds: PlanetMapBounds {
                    lon_min: cal.bounds.lon_min,
                    lon_max: cal.bounds.lon_max,
                    lat_min: cal.bounds.lat_min,
                    lat_max: cal.bounds.lat_max,
                },
            })
            .into(),
    }
}
