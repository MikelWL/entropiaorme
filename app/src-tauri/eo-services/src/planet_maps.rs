//! Bundled planet-map catalogue: per-planet map rasters plus the
//! tile-grid calibration that places game coordinates on them.
//!
//! The bundle (`resources/maps/`) holds one raster image per
//! planet/instance and a `calibration.json` describing each map as an
//! axis-aligned window onto the game's global tile grid (one tile is
//! 8192 x 8192 game units): a tile origin (the map's south-west corner)
//! plus an extent in tiles. Positions are `(longitude, latitude)` with
//! longitude growing eastward (image x) and latitude growing northward
//! (opposite of image y). The store parses the calibration once at
//! construction, derives per-axis game-units-per-pixel scales (a map
//! whose aspect ratio does not match its tile window needs a distinct
//! vertical scale), verifies each record's image file exists, and
//! serves records and image bytes from memory-held metadata plus
//! on-demand file reads.
//!
//! Failure posture mirrors the other optional bundled assets: a missing
//! bundle directory yields an empty store (warn-and-continue; the maps
//! surface stands down), while an unreadable or unparseable calibration
//! file is a hard error (a broken bundle is a packaging defect, not a
//! runtime state). A record whose image is missing or unrecognised is
//! dropped with a warning; a record without calibration is retained
//! (its map can be displayed, but coordinates cannot be placed on it).

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The game's global tile size in game units per side. The bundle's own
/// `gameUnitsPerTile` is validated against this at load.
pub const GAME_UNITS_PER_TILE: i64 = 8192;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalibrationFile {
    game_units_per_tile: i64,
    planets: Vec<RawPlanet>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlanet {
    name: String,
    technical_name: Option<String>,
    image: String,
    image_width_px: u32,
    image_height_px: u32,
    tile_origin_x: Option<i64>,
    tile_origin_y: Option<i64>,
    tile_width: Option<i64>,
    tile_height: Option<i64>,
    bounds: Option<RawBounds>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBounds {
    lon_min: i64,
    lon_max: i64,
    lat_min: i64,
    lat_max: i64,
}

/// A map's coordinate window in game units: the plausibility gate for
/// any coordinate claimed to lie on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapBounds {
    pub lon_min: i64,
    pub lon_max: i64,
    pub lat_min: i64,
    pub lat_max: i64,
}

impl MapBounds {
    /// Whether a coordinate pair lies inside this window (inclusive).
    pub fn contains(&self, lon: i64, lat: i64) -> bool {
        (self.lon_min..=self.lon_max).contains(&lon) && (self.lat_min..=self.lat_max).contains(&lat)
    }
}

/// A calibrated map's placement on the global tile grid, with the
/// derived per-axis scales. Present only when the bundle knows where
/// the map sits; an uncalibrated map is view-only.
#[derive(Debug, Clone, PartialEq)]
pub struct MapCalibration {
    pub tile_origin_x: i64,
    pub tile_origin_y: i64,
    pub tile_width: i64,
    pub tile_height: i64,
    /// Game units per image pixel along x (from the tile width). Most
    /// maps are isotropic; a map whose raster aspect ratio differs from
    /// its tile window is not, hence the per-axis pair.
    pub units_per_pixel_x: f64,
    /// Game units per image pixel along y (from the tile height).
    pub units_per_pixel_y: f64,
    pub bounds: MapBounds,
}

/// One planet/instance map: display metadata plus optional calibration.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanetMapRecord {
    pub name: String,
    /// The planet name the in-game waypoint syntax accepts, when known.
    pub technical_name: Option<String>,
    /// The image's file name inside the bundle directory (bundle-owned,
    /// never caller-supplied; consumers address maps by planet name).
    pub image_file: String,
    /// The raster's MIME type, from its extension.
    pub image_mime: &'static str,
    pub image_width_px: u32,
    pub image_height_px: u32,
    pub calibration: Option<MapCalibration>,
}

/// The loaded catalogue: records in bundle order, image bytes read on
/// demand from the bundle directory.
pub struct PlanetMapStore {
    maps_dir: PathBuf,
    records: Vec<PlanetMapRecord>,
}

impl PlanetMapStore {
    /// Load the bundle at `maps_dir`. A missing directory (or missing
    /// `calibration.json`) yields an empty store; an unreadable or
    /// unparseable calibration file is a hard error.
    pub fn new(maps_dir: &Path) -> std::io::Result<Self> {
        let calibration_path = maps_dir.join("calibration.json");
        if !calibration_path.is_file() {
            tracing::warn!(
                target: "eo::planet_maps",
                "planet-map bundle at {} is absent; the maps surface stands down",
                maps_dir.display()
            );
            return Ok(Self {
                maps_dir: maps_dir.to_path_buf(),
                records: Vec::new(),
            });
        }
        let raw = std::fs::read_to_string(&calibration_path)?;
        let file = serde_json::from_str::<CalibrationFile>(&raw).map_err(|e| {
            std::io::Error::other(format!(
                "planet-map calibration {} does not parse: {e}",
                calibration_path.display()
            ))
        })?;
        if file.game_units_per_tile != GAME_UNITS_PER_TILE {
            return Err(std::io::Error::other(format!(
                "planet-map calibration declares {} game units per tile; expected {}",
                file.game_units_per_tile, GAME_UNITS_PER_TILE
            )));
        }
        let records = file
            .planets
            .into_iter()
            .filter_map(|planet| build_record(maps_dir, planet))
            .collect();
        Ok(Self {
            maps_dir: maps_dir.to_path_buf(),
            records,
        })
    }

    /// Every loaded map, in bundle order.
    pub fn records(&self) -> &[PlanetMapRecord] {
        &self.records
    }

    /// The record for a planet, by display name.
    pub fn record(&self, planet_name: &str) -> Option<&PlanetMapRecord> {
        self.records
            .iter()
            .find(|record| record.name == planet_name)
    }

    /// A planet's raster bytes, read from the bundle. `None` for an
    /// unknown planet or an unreadable file (logged).
    pub fn image_bytes(&self, planet_name: &str) -> Option<Vec<u8>> {
        let record = self.record(planet_name)?;
        match std::fs::read(self.maps_dir.join(&record.image_file)) {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                tracing::warn!(
                    target: "eo::planet_maps",
                    "planet map {} unreadable ({err})",
                    record.image_file
                );
                None
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Validate one raw record into a served one, or drop it with a warning.
fn build_record(maps_dir: &Path, planet: RawPlanet) -> Option<PlanetMapRecord> {
    // The image field is bundle-authored, but hold it to a bare file
    // name anyway so a malformed bundle can never read outside its dir.
    let image_file = match Path::new(&planet.image)
        .file_name()
        .and_then(|f| f.to_str())
    {
        Some(file) if !file.is_empty() => file.to_string(),
        _ => {
            tracing::warn!(
                target: "eo::planet_maps",
                "planet map {} has no usable image file name; record dropped",
                planet.name
            );
            return None;
        }
    };
    let image_mime = match Path::new(&image_file)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => {
            tracing::warn!(
                target: "eo::planet_maps",
                "planet map {image_file} has an unrecognised image format; record dropped"
            );
            return None;
        }
    };
    if !maps_dir.join(&image_file).is_file() {
        tracing::warn!(
            target: "eo::planet_maps",
            "planet map image {image_file} is missing from the bundle; record dropped"
        );
        return None;
    }
    if planet.image_width_px == 0 || planet.image_height_px == 0 {
        tracing::warn!(
            target: "eo::planet_maps",
            "planet map {image_file} declares a degenerate raster size; record dropped"
        );
        return None;
    }
    let calibration = build_calibration(&planet);
    Some(PlanetMapRecord {
        name: planet.name,
        technical_name: planet.technical_name,
        image_file,
        image_mime,
        image_width_px: planet.image_width_px,
        image_height_px: planet.image_height_px,
        calibration,
    })
}

/// Derive a record's calibration from its tile window, or `None` when
/// any part is absent (an uncalibrated, view-only map) or inconsistent
/// (logged and treated as uncalibrated rather than served wrong).
fn build_calibration(planet: &RawPlanet) -> Option<MapCalibration> {
    let (origin_x, origin_y, tiles_w, tiles_h, raw_bounds) = match (
        planet.tile_origin_x,
        planet.tile_origin_y,
        planet.tile_width,
        planet.tile_height,
        planet.bounds.as_ref(),
    ) {
        (Some(ox), Some(oy), Some(tw), Some(th), Some(bounds)) => (ox, oy, tw, th, bounds),
        (None, None, None, None, None) => return None,
        _ => {
            tracing::warn!(
                target: "eo::planet_maps",
                "planet map {} has partial calibration; treated as view-only",
                planet.name
            );
            return None;
        }
    };
    if tiles_w <= 0 || tiles_h <= 0 {
        tracing::warn!(
            target: "eo::planet_maps",
            "planet map {} declares a degenerate tile window; treated as view-only",
            planet.name
        );
        return None;
    }
    let bounds = MapBounds {
        lon_min: raw_bounds.lon_min,
        lon_max: raw_bounds.lon_max,
        lat_min: raw_bounds.lat_min,
        lat_max: raw_bounds.lat_max,
    };
    let derived = MapBounds {
        lon_min: origin_x * GAME_UNITS_PER_TILE,
        lon_max: (origin_x + tiles_w) * GAME_UNITS_PER_TILE,
        lat_min: origin_y * GAME_UNITS_PER_TILE,
        lat_max: (origin_y + tiles_h) * GAME_UNITS_PER_TILE,
    };
    if bounds != derived {
        tracing::warn!(
            target: "eo::planet_maps",
            "planet map {} bounds disagree with its tile window; treated as view-only",
            planet.name
        );
        return None;
    }
    Some(MapCalibration {
        tile_origin_x: origin_x,
        tile_origin_y: origin_y,
        tile_width: tiles_w,
        tile_height: tiles_h,
        units_per_pixel_x: (tiles_w * GAME_UNITS_PER_TILE) as f64
            / f64::from(planet.image_width_px),
        units_per_pixel_y: (tiles_h * GAME_UNITS_PER_TILE) as f64
            / f64::from(planet.image_height_px),
        bounds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo's bundled maps directory (the dev-layout resolution).
    fn bundled_maps_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("entropia-orme")
            .join("resources")
            .join("maps")
    }

    #[test]
    fn a_missing_bundle_yields_an_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlanetMapStore::new(dir.path()).unwrap();
        assert!(store.is_empty());
        assert!(store.image_bytes("Calypso").is_none());
    }

    #[test]
    fn a_malformed_calibration_is_a_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("calibration.json"), "{not json").unwrap();
        assert!(PlanetMapStore::new(dir.path()).is_err());
    }

    #[test]
    fn a_wrong_tile_constant_is_a_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("calibration.json"),
            r#"{"gameUnitsPerTile": 4096, "planets": []}"#,
        )
        .unwrap();
        assert!(PlanetMapStore::new(dir.path()).is_err());
    }

    #[test]
    fn the_bundled_catalogue_loads_with_its_known_shape() {
        let store = PlanetMapStore::new(&bundled_maps_dir()).unwrap();
        // 20 shipped maps: 19 calibrated plus the view-only Thule.
        assert_eq!(store.records().len(), 20);
        assert_eq!(
            store
                .records()
                .iter()
                .filter(|r| r.calibration.is_some())
                .count(),
            19
        );
        let thule = store.record("Thule").unwrap();
        assert!(thule.calibration.is_none());
    }

    #[test]
    fn calypso_carries_the_specced_calibration() {
        let store = PlanetMapStore::new(&bundled_maps_dir()).unwrap();
        let calypso = store.record("Calypso").unwrap();
        assert_eq!(calypso.image_mime, "image/jpeg");
        assert_eq!(calypso.image_width_px, 4608);
        let cal = calypso.calibration.as_ref().unwrap();
        assert_eq!((cal.tile_origin_x, cal.tile_origin_y), (2, 3));
        assert_eq!((cal.tile_width, cal.tile_height), (9, 9));
        assert_eq!(cal.units_per_pixel_x, 16.0);
        assert_eq!(cal.units_per_pixel_y, 16.0);
        assert_eq!(
            cal.bounds,
            MapBounds {
                lon_min: 16384,
                lon_max: 90112,
                lat_min: 24576,
                lat_max: 98304,
            }
        );
        // The sanity anchor: Port Atlantis (~61400, ~75800) lies inside.
        assert!(cal.bounds.contains(61400, 75800));
        assert!(!cal.bounds.contains(61400, 99999));
    }

    /// ARIS's raster aspect ratio does not match its tile window, the
    /// case the per-axis scales exist for.
    #[test]
    fn an_anisotropic_map_gets_per_axis_scales() {
        let store = PlanetMapStore::new(&bundled_maps_dir()).unwrap();
        let aris = store.record("ARIS").unwrap();
        let cal = aris.calibration.as_ref().unwrap();
        assert_eq!(cal.units_per_pixel_x, 48.0);
        assert!((cal.units_per_pixel_y - 21.333_333).abs() < 1e-3);
    }

    #[test]
    fn image_bytes_serves_the_bundle_by_planet_name() {
        let store = PlanetMapStore::new(&bundled_maps_dir()).unwrap();
        let bytes = store.image_bytes("Calypso").unwrap();
        // JPEG magic: a real raster came back, not an error page.
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);
        assert!(store.image_bytes("Atlantis").is_none());
    }
}
