/**
 * The maps view model: the bundled planet catalogue, the selected
 * planet with its raster (fetched as a `data:` URL through the shell
 * byte command), and the selected planet's pins with their CRUD verbs.
 * Presentation and viewport interaction live in the components; this
 * module owns the data and its loading/error state.
 */

import type { MapPin, MapPinInput, MapPinPatch, MapView, PlanetMap } from '$lib/api';
import {
	createMapPin,
	createMapView,
	deleteMapPin,
	deleteMapView,
	getMapPins,
	getMapViews,
	getPlanetMaps,
	planetMapImage,
	renameMapView,
	updateMapPin,
} from '$lib/api';
import { describeError } from '$lib/view/errorState';

export function createMapsModel() {
	let planets = $state<PlanetMap[]>([]);
	let selectedName = $state<string | null>(null);
	let imageUrl = $state<string | null>(null);
	let views = $state<MapView[]>([]);
	let selectedViewId = $state<number | null>(null);
	let pins = $state<MapPin[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Selection is async (raster fetch + pin load); the epoch guard keeps
	// a stale planet's late arrival from clobbering the current one.
	let selectEpoch = 0;

	const selected = $derived(planets.find((planet) => planet.name === selectedName) ?? null);

	async function loadPlanets() {
		loading = true;
		error = null;
		try {
			planets = await getPlanetMaps();
			if (planets.length > 0) {
				await selectPlanet(planets[0].name);
			} else {
				loading = false;
			}
		} catch (e) {
			error = describeError(e, 'Failed to load the planet maps');
			loading = false;
		}
	}

	async function refreshPins(): Promise<void> {
		const planet = selectedName;
		if (!planet) return;
		const epoch = selectEpoch;
		try {
			const loadedPins = await getMapPins(planet, selectedViewId);
			if (epoch === selectEpoch && selectedName === planet) pins = loadedPins;
		} catch (e) {
			if (epoch === selectEpoch) error = describeError(e, `Failed to refresh the ${planet} pins`);
		}
	}

	async function selectPlanet(name: string) {
		const epoch = ++selectEpoch;
		const planet = planets.find((candidate) => candidate.name === name);
		if (!planet) return;
		selectedName = name;
		selectedViewId = null;
		imageUrl = null;
		views = [];
		pins = [];
		loading = true;
		error = null;
		try {
			const [url, loadedViews, loadedPins] = await Promise.all([
				planetMapImage(planet.name, planet.imageMime),
				getMapViews(planet.name),
				getMapPins(planet.name, null),
			]);
			if (epoch !== selectEpoch) return;
			imageUrl = url;
			views = loadedViews;
			pins = loadedPins;
		} catch (e) {
			if (epoch !== selectEpoch) return;
			error = describeError(e, `Failed to load the ${name} map`);
		} finally {
			if (epoch === selectEpoch) loading = false;
		}
	}

	async function selectView(id: number | null) {
		const planet = selectedName;
		if (!planet || (id !== null && !views.some((view) => view.id === id))) return;
		const epoch = ++selectEpoch;
		selectedViewId = id;
		pins = [];
		error = null;
		try {
			const loadedPins = await getMapPins(planet, id);
			if (epoch === selectEpoch && selectedName === planet && selectedViewId === id) {
				pins = loadedPins;
			}
		} catch (e) {
			if (epoch === selectEpoch) error = describeError(e, `Failed to load the ${planet} map view`);
		}
	}

	async function addView(): Promise<MapView | null> {
		if (!selectedName) return null;
		let suffix = 1;
		let name = 'New map';
		const names = new Set(views.map((view) => view.name.toLowerCase()));
		while (names.has(name.toLowerCase())) name = `New map ${++suffix}`;
		const created = await createMapView(selectedName, name);
		views = [...views, created];
		await selectView(created.id);
		return created;
	}

	async function renameView(id: number, name: string): Promise<MapView> {
		const renamed = await renameMapView(id, name);
		views = views.map((view) => (view.id === id ? renamed : view));
		return renamed;
	}

	async function removeView(id: number): Promise<void> {
		await deleteMapView(id);
		views = views.filter((view) => view.id !== id);
		if (selectedViewId === id) await selectView(null);
	}

	async function addPin(input: MapPinInput): Promise<MapPin> {
		const created = await createMapPin(input);
		if (created.planet === selectedName && created.mapViewId === selectedViewId) {
			pins = [created, ...pins];
		}
		return created;
	}

	async function editPin(id: number, patch: MapPinPatch): Promise<MapPin> {
		const updated = await updateMapPin(id, patch);
		pins = pins.map((pin) => (pin.id === id ? updated : pin));
		return updated;
	}

	async function removePin(id: number): Promise<void> {
		await deleteMapPin(id);
		pins = pins.filter((pin) => pin.id !== id);
	}

	return {
		get planets() {
			return planets;
		},
		get selected() {
			return selected;
		},
		get imageUrl() {
			return imageUrl;
		},
		get pins() {
			return pins;
		},
		get views() {
			return views;
		},
		get selectedViewId() {
			return selectedViewId;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		loadPlanets,
		selectPlanet,
		selectView,
		addView,
		renameView,
		removeView,
		refreshPins,
		addPin,
		editPin,
		removePin,
	};
}

export type MapsModel = ReturnType<typeof createMapsModel>;
