//! Auction listing-fee economics measured from the game's sale window.
//!
//! The fee depends on gross markup, not item identity, quantity, buyout, or
//! duration. A fee-efficient packet keeps the listing fee within the user's
//! chosen share of expected gross markup, before the separately charged sale
//! fee.

const CURVE_COEFFICIENT: f64 = 0.000_670_02;
const BASE_FEE_PED: f64 = 0.50;
const PEC_PER_PED: f64 = 100.0;
// The curve approaches 75.124... PED from below, so 75.12 PED is the
// greatest fee the game's whole-PEC display can emit.
const MAX_DISPLAYED_FEE_PEC: u64 = 7_512;
pub const DEFAULT_MAX_FEE_SHARE_PCT: f64 = 10.0;

/// The game's displayed listing fee for a non-negative gross markup amount.
/// The quote truncates, rather than rounds, to whole PEC.
pub fn listing_fee_ped(gross_markup_ped: f64) -> f64 {
    if !gross_markup_ped.is_finite() || gross_markup_ped <= 0.0 {
        return BASE_FEE_PED;
    }
    let raw = BASE_FEE_PED + 0.05 / (CURVE_COEFFICIENT + gross_markup_ped.recip());
    (raw * PEC_PER_PED + f64::EPSILON).floor() / PEC_PER_PED
}

/// Smallest TT packet, rounded upward to a whole PEC, whose expected direct
/// market markup amortises the listing fee to the requested share or less.
///
/// Markup is the auction's percentage convention (`105` means five per cent
/// over TT). No recommendation is possible without a finite positive premium.
pub fn recommended_packet_tt(markup_pct: f64, max_fee_share_pct: f64) -> Option<f64> {
    if !markup_pct.is_finite() || markup_pct <= 100.0 {
        return None;
    }
    let premium = markup_pct / 100.0 - 1.0;
    let required_markup = minimum_efficient_gross_markup_ped(max_fee_share_pct)?;
    // Remove only floating-point dust at an exact PEC boundary. This does not
    // turn a genuinely fractional PEC downwards.
    Some((((required_markup / premium) * PEC_PER_PED) - 1e-9).ceil() / PEC_PER_PED)
}

pub fn minimum_efficient_gross_markup_ped(max_fee_share_pct: f64) -> Option<f64> {
    if !max_fee_share_pct.is_finite() || max_fee_share_pct <= 0.0 || max_fee_share_pct > 100.0 {
        return None;
    }
    let fee_share = max_fee_share_pct / 100.0;

    // Truncating the displayed fee creates a small sawtooth at every fee-PEC
    // boundary, so efficiency is not a monotonic predicate and cannot be
    // binary-searched. Instead, consider every possible displayed fee. For a
    // fee F, ceil(F / share) is the earliest markup PEC that could qualify;
    // if the actual fee there is no greater than F, it is efficient. Any true
    // global minimum must appear among these finite candidates.
    for displayed_fee_pec in 50..=MAX_DISPLAYED_FEE_PEC {
        let candidate = ((displayed_fee_pec as f64 / fee_share) - 1e-9).ceil();
        if !candidate.is_finite() || candidate > u64::MAX as f64 {
            return None;
        }
        let candidate_pec = candidate.max(1.0) as u64;
        let gross_markup = candidate_pec as f64 / PEC_PER_PED;
        if listing_fee_ped(gross_markup) <= fee_share * gross_markup {
            return Some(gross_markup);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_curve_points_match_the_game_to_the_pec() {
        for (markup, fee) in [
            (0.0, 0.50),
            (1.0, 0.54),
            (100.0, 5.18),
            (900.0, 28.57),
            (9_900.0, 65.34),
            (1_111_118.96, 75.02),
        ] {
            assert_eq!(listing_fee_ped(markup), fee, "gross markup {markup}");
        }
    }

    #[test]
    fn finite_markup_extremes_reach_the_displayed_fee_cap() {
        assert_eq!(listing_fee_ped(f64::MAX), 75.12);
    }

    #[test]
    fn fee_efficiency_crosses_at_nine_ped_eighty_markup() {
        assert!(listing_fee_ped(9.79) > 9.79 * 0.10);
        assert_eq!(listing_fee_ped(9.80), 0.98);
        assert!(listing_fee_ped(9.80) <= 9.80 * 0.10);
        assert!(listing_fee_ped(125.99) > 125.99 * 0.05);
        assert_eq!(listing_fee_ped(126.00), 6.30);
        assert!(listing_fee_ped(126.00) <= 126.00 * 0.05);
        assert_eq!(minimum_efficient_gross_markup_ped(5.0), Some(126.0));
        assert_eq!(minimum_efficient_gross_markup_ped(10.0), Some(9.8));
        assert_eq!(minimum_efficient_gross_markup_ped(15.0), Some(4.94));
    }

    #[test]
    fn packet_recommendations_scale_with_the_market_premium() {
        assert_eq!(recommended_packet_tt(105.0, 10.0), Some(196.0));
        assert_eq!(recommended_packet_tt(110.0, 10.0), Some(98.0));
        assert_eq!(recommended_packet_tt(120.0, 10.0), Some(49.0));
        assert_eq!(recommended_packet_tt(200.0, 10.0), Some(9.80));
        assert_eq!(recommended_packet_tt(100.0, 10.0), None);
        assert_eq!(recommended_packet_tt(f64::NAN, 10.0), None);
        assert_eq!(recommended_packet_tt(110.0, 0.0), None);
    }
}
