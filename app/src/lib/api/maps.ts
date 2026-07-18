/**
 * The maps family: the bundled planet-map catalogue and the cartography
 * pins on it. Thin wrappers over the generated typed commands; the map
 * raster itself rides the bespoke shell command (`planetMapImage` in
 * `./shell`), since raw image bytes cannot cross the typed-DTO surface.
 */

import * as commands from './commands.gen';

export type {
	CoordCalibrationStatus,
	CoordScanResult,
	MapPin,
	MapPinInput,
	MapPinPatch,
	MapView,
	PlanetMap,
	PlanetMapBounds,
	PlanetMapCalibration,
} from './commands.gen';

export const getPlanetMaps = commands.planetMapsList;
export const getMapPins = commands.mapPinsList;
export const getMapViews = commands.mapViewsList;
export const createMapView = commands.mapViewCreate;
export const renameMapView = commands.mapViewRename;
export const deleteMapView = commands.mapViewDelete;
export const createMapPin = commands.mapPinCreate;
export const updateMapPin = commands.mapPinUpdate;
export const deleteMapPin = commands.mapPinDelete;
export const startMapsCalibration = commands.mapsCalibrationStart;
export const cancelMapsCalibration = commands.mapsCalibrationCancel;
export const getMapsCalibrationStatus = commands.mapsCalibrationStatus;
export const scanMapCoordinates = commands.mapsScanCoordinates;
