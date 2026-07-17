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

use schemars::JsonSchema;
use serde::Serialize;

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
