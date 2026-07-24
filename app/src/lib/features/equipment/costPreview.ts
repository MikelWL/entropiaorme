/**
 * Local cost-per-use preview for the equipment form. The formula mirrors the
 * backend cost engine's per-use pricing (eo-services/src/cost_engine.rs) for
 * instant feedback while the form is edited: markup applies to decay only
 * (percent / 100, limited items only), each damage-enhancer slot adds 10% to
 * weapon decay and ammo, the amp shares the weapon's markup input, and the
 * manual decay-split devices take their shares of the weapon's decay first
 * (implant, then extender on the remainder) at their own markups. The preview
 * covers weapon, amp, scope and the split devices; absorbers and consumables
 * do not move it. The stored entry's authoritative cost comes back from the
 * save.
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
	/** Implant share of the weapon's decay, percent; null or 0 = no implant. */
	implantSharePercent?: number | null;
	implantMarkupPercent?: number;
	/** Extender share of the post-implant decay, percent; null or 0 = none. */
	extenderAbsorptionPercent?: number | null;
	extenderMarkupPercent?: number;
}

/** Estimated cost per use in PEC, or null while no weapon is selected. */
export function previewCostPerUse(input: CostPreviewInput): number | null {
	const { weapon, amp, scope, markupPercent, scopeMarkupPercent, damageEnhancers } = input;
	if (!weapon) return null;
	const weaponMult = weapon.isLimited ? markupPercent / 100 : 1.0;
	const enhancerMult = 1 + damageEnhancers * 0.1;
	const implantShare = Math.min(Math.max((input.implantSharePercent ?? 0) / 100, 0), 1);
	const extenderShare = Math.min(Math.max((input.extenderAbsorptionPercent ?? 0) / 100, 0), 1);
	const scaledDecay = weapon.decay * enhancerMult;
	const implantDecay = scaledDecay * implantShare;
	const extenderDecay = (scaledDecay - implantDecay) * extenderShare;
	const weaponDecay = scaledDecay - implantDecay - extenderDecay;
	let cost =
		weaponDecay * weaponMult +
		implantDecay * ((input.implantMarkupPercent ?? 100) / 100) +
		extenderDecay * ((input.extenderMarkupPercent ?? 100) / 100) +
		weapon.ammoBurn * enhancerMult;
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
