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
	NavigationPositionResult,
	NavigationPositionStatus,
	NavigationRun,
	PinConfig,
	PinConfigEditInput,
	PinConfigInput,
	NavigationRunStatus,
	NavigationStop,
	NavigationStopStatus,
	NearbyMapPin,
	PlanetMap,
	PlanetMapBounds,
	PlanetMapCalibration,
	RadarCalibrationStatus,
	RadarGeometry,
} from './commands.gen';

export const getPlanetMaps = commands.planetMapsList;
export const getMapPins = commands.mapPinsList;
export const getMapPinsInViewport = commands.mapPinsViewport;
export const getNearbyMapPin = commands.mapPinNearby;
export const getMapViews = commands.mapViewsList;
export const createMapView = commands.mapViewCreate;
export const renameMapView = commands.mapViewRename;
export const deleteMapView = commands.mapViewDelete;
export const createMapPin = commands.mapPinCreate;
export const updateMapPin = commands.mapPinUpdate;
export const deleteMapPin = commands.mapPinDelete;
export const getPinConfigs = commands.pinConfigsList;
export const createPinConfig = commands.pinConfigCreate;
export const updatePinConfig = commands.pinConfigUpdate;
export const deletePinConfig = commands.pinConfigDelete;
export const reorderPinConfigs = commands.pinConfigReorder;
export const startMapsCalibration = commands.mapsCalibrationStart;
export const cancelMapsCalibration = commands.mapsCalibrationCancel;
export const getMapsCalibrationStatus = commands.mapsCalibrationStatus;
export const scanMapCoordinates = commands.mapsScanCoordinates;
export const getNavigationSnapshot = commands.navigationSnapshot;
export const startNavigation = commands.navigationStart;
export const updateNavigationPosition = commands.navigationUpdatePosition;
export const markNavigationVisited = commands.navigationMarkVisited;
export const skipNavigationStop = commands.navigationSkip;
export const resolveNavigationHarvest = commands.navigationResolveHarvest;
export const undoNavigationStop = commands.navigationUndo;
export const endNavigation = commands.navigationEnd;
export const startRadarCalibration = commands.radarCalibrationStart;
export const cancelRadarCalibration = commands.radarCalibrationCancel;
export const getRadarCalibrationStatus = commands.radarCalibrationStatus;
export const getRadarGeometry = commands.radarGeometry;
