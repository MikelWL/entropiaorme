/**
 * Local cost-per-use preview for the equipment form. The formula mirrors the
 * backend cost engine's per-use pricing (eo-services/src/cost_engine.rs) for
 * instant feedback while the form is edited: markup applies to decay only
 * (percent / 100, limited items only), each damage-enhancer slot adds 10% to
 * weapon decay and ammo, and the amp shares the weapon's markup input. The
 * preview covers weapon, amp and scope; absorbers and consumables do not move
 * it. The stored entry's authoritative cost comes back from the save.
 */

/** The three economy fields the preview reads from a catalogue selection. */
export interface PreviewComponent {
	/** Decay per use, PEC. */
	decay: number;
	/** Ammo burn per use, PEC. */
	ammoBurn: number;
	isLimited: boolean;
}

export interface CostPreviewInput {
	weapon: PreviewComponent | null;
	amp: PreviewComponent | null;
	scope: PreviewComponent | null;
	/** Shared markup percent for the weapon and amp (limited items only). */
	markupPercent: number;
	scopeMarkupPercent: number;
	damageEnhancers: number;
}

/** Estimated cost per use in PEC, or null while no weapon is selected. */
export function previewCostPerUse(input: CostPreviewInput): number | null {
	const { weapon, amp, scope, markupPercent, scopeMarkupPercent, damageEnhancers } = input;
	if (!weapon) return null;
	const weaponMult = weapon.isLimited ? markupPercent / 100 : 1.0;
	const enhancerMult = 1 + damageEnhancers * 0.1;
	let cost = weapon.decay * weaponMult * enhancerMult + weapon.ammoBurn * enhancerMult;
	if (amp) {
		const ampMult = amp.isLimited ? markupPercent / 100 : 1.0;
		cost += amp.decay * ampMult + amp.ammoBurn;
	}
	if (scope) {
		const scopeMult = scope.isLimited ? scopeMarkupPercent / 100 : 1.0;
		cost += scope.decay * scopeMult;
	}
	return cost;
}
