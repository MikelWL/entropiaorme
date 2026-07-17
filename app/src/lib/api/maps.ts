/**
 * The maps family: the bundled planet-map catalogue and the cartography
 * pins on it. Thin wrappers over the generated typed commands; the map
 * raster itself rides the bespoke shell command (`planetMapImage` in
 * `./shell`), since raw image bytes cannot cross the typed-DTO surface.
 */

import * as commands from './commands.gen';

export type {
	MapPin,
	MapPinInput,
	MapPinPatch,
	PlanetMap,
	PlanetMapBounds,
	PlanetMapCalibration,
} from './commands.gen';

export const getPlanetMaps = commands.planetMapsList;
export const getMapPins = commands.mapPinsList;
export const createMapPin = commands.mapPinCreate;
export const updateMapPin = commands.mapPinUpdate;
export const deleteMapPin = commands.mapPinDelete;
