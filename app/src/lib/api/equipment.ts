/**
 * The equipment family: catalogue search, the library, and the
 * per-item detail. Thin wrappers over the generated typed commands;
 * argument shaping only.
 */

import type { EquipmentSearchHit, SearchKind } from './commands.gen';
import * as commands from './commands.gen';

/** Search result from the equipment catalogue search command. The two
 * optional fields are not part of the wire shape: the equipment page
 * reuses this type to seed its selection state from a stored detail,
 * which carries them. */
export type EquipmentSearchResult = EquipmentSearchHit & {
	markupPercent?: number;
	damageEnhancers?: number;
};

export const addToLibrary = commands.equipmentAdd;
export const getEquipmentLibrary = commands.equipmentLibrary;

export async function searchEquipmentItems(
	q: string,
	type: SearchKind,
): Promise<EquipmentSearchResult[]> {
	if (q.length < 2) return [];
	return commands.equipmentSearch(q, type);
}

export async function removeFromLibrary(id: string): Promise<void> {
	await commands.equipmentDelete(Number(id));
}

export async function updateLibrary(id: string, req: commands.EquipmentRequest) {
	return commands.equipmentUpdate(Number(id), req);
}

export async function getEquipmentDetail(id: string) {
	return commands.equipmentDetail(Number(id));
}
