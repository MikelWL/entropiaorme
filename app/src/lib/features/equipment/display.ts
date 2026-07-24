/**
 * Equipment display formatting: the enrichment-level ladder and the PEC
 * amount format the library rows, detail panels and form preview share.
 */

export type EnrichmentColor = 'negative' | 'warning' | 'accent' | 'positive';

// The wire carries the enrichment level as a plain number (0-3 by
// construction); anything outside the ladder reads as unresolved.
export function enrichmentLabel(level: number): string {
	const labels = ['Unresolved', 'Base', 'Base + Amp', 'Full Setup'];
	return labels[level] ?? labels[0];
}

export function enrichmentColor(level: number): EnrichmentColor {
	const colors: EnrichmentColor[] = ['negative', 'warning', 'accent', 'positive'];
	return colors[level] ?? colors[0];
}

export function formatPec(pec: number): string {
	// Two decimals reads best at ordinary magnitudes, but very slow decays
	// (below 0.1 PEC per use) would collapse to "0.01" or "0.00"; those get
	// four decimals so the figure stays honest.
	return pec.toFixed(pec !== 0 && Math.abs(pec) < 0.1 ? 4 : 2);
}
