import { describe, expect, it, vi } from 'vitest';
import { createMapPinSelectionController } from './mapPinSelectionController.svelte';
import type { MapsModel } from './mapsModel.svelte';

function model() {
	return {
		removePins: vi.fn().mockResolvedValue(undefined),
		cooldownPins: vi.fn().mockResolvedValue(undefined),
	} as unknown as MapsModel;
}

describe('map pin area-selection controller', () => {
	it('requires confirmation before deleting the unique selected pins', async () => {
		const maps = model();
		const flash = vi.fn();
		const confirmDelete = vi.fn().mockReturnValue(true);
		const controller = createMapPinSelectionController(maps, flash, confirmDelete);
		controller.begin();
		controller.setRegions([{ left: 1, top: 2, right: 3, bottom: 4 }]);

		await controller.deleteSelected([4, 2, 4]);

		expect(confirmDelete).toHaveBeenCalledWith('Delete these 2 pins? This cannot be undone.');
		expect(maps.removePins).toHaveBeenCalledWith([4, 2]);
		expect(flash).toHaveBeenCalledWith('2 pins deleted.');
		expect(controller.active).toBe(false);
		expect(controller.regions).toEqual([]);
	});

	it('keeps the selection when deletion is declined', async () => {
		const maps = model();
		const controller = createMapPinSelectionController(maps, vi.fn(), () => false);
		controller.begin();
		controller.setRegions([{ left: 1, top: 2, right: 3, bottom: 4 }]);

		await controller.deleteSelected([2]);

		expect(maps.removePins).not.toHaveBeenCalled();
		expect(controller.active).toBe(true);
		expect(controller.regions).toHaveLength(1);
	});

	it('puts the selected tree ids on cooldown and exits selection', async () => {
		const maps = model();
		const flash = vi.fn();
		const controller = createMapPinSelectionController(maps, flash);
		controller.begin();

		await controller.cooldownSelected([8, 3]);

		expect(maps.cooldownPins).toHaveBeenCalledWith([8, 3]);
		expect(flash).toHaveBeenCalledWith('2 trees put on cooldown.');
		expect(controller.active).toBe(false);
	});
});
