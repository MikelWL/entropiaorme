//! Auction listing-fee economics measured from the game's sale window.
//!
//! The fee depends on gross markup, not item identity, quantity, buyout, or
//! duration. A fee-efficient packet keeps the listing fee to at most ten per
//! cent of its expected gross markup, preserving at least ninety per cent of
//! that markup before the separately charged sale fee.

const CURVE_COEFFICIENT: f64 = 0.000_670_02;
const BASE_FEE_PED: f64 = 0.50;
const HEALTHY_FEE_SHARE: f64 = 0.10;
const PEC_PER_PED: f64 = 100.0;

/// The game's displayed listing fee for a non-negative gross markup amount.
/// The quote truncates, rather than rounds, to whole PEC.
pub fn listing_fee_ped(gross_markup_ped: f64) -> f64 {
    if !gross_markup_ped.is_finite() || gross_markup_ped <= 0.0 {
        return BASE_FEE_PED;
    }
    let raw =
        BASE_FEE_PED + (0.05 * gross_markup_ped) / (1.0 + CURVE_COEFFICIENT * gross_markup_ped);
    (raw * PEC_PER_PED + f64::EPSILON).floor() / PEC_PER_PED
}

/// Smallest TT packet, rounded upward to a whole PEC, whose expected direct
/// market markup amortises the listing fee to ten per cent or less.
///
/// Markup is the auction's percentage convention (`105` means five per cent
/// over TT). No recommendation is possible without a finite positive premium.
pub fn recommended_packet_tt(markup_pct: f64) -> Option<f64> {
    if !markup_pct.is_finite() || markup_pct <= 100.0 {
        return None;
    }
    let premium = markup_pct / 100.0 - 1.0;
    let required_markup = minimum_efficient_gross_markup_ped();
    // Remove only floating-point dust at an exact PEC boundary. This does not
    // turn a genuinely fractional PEC downwards.
    Some((((required_markup / premium) * PEC_PER_PED) - 1e-9).ceil() / PEC_PER_PED)
}

fn minimum_efficient_gross_markup_ped() -> f64 {
    // Search in the same whole-PEC grain the game displays. The bound is
    // deliberately generous relative to the measured 9.80 PED crossing.
    for gross_markup_pec in 1..=100_000 {
        let gross_markup = f64::from(gross_markup_pec) / PEC_PER_PED;
        if listing_fee_ped(gross_markup) <= HEALTHY_FEE_SHARE * gross_markup {
            return gross_markup;
        }
    }
    unreachable!("the saturating auction fee must cross the healthy fee share")
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
    fn fee_efficiency_crosses_at_nine_ped_eighty_markup() {
        assert!(listing_fee_ped(9.79) > 9.79 * HEALTHY_FEE_SHARE);
        assert_eq!(listing_fee_ped(9.80), 0.98);
        assert!(listing_fee_ped(9.80) <= 9.80 * HEALTHY_FEE_SHARE);
    }

    #[test]
    fn packet_recommendations_scale_with_the_market_premium() {
        assert_eq!(recommended_packet_tt(105.0), Some(196.0));
        assert_eq!(recommended_packet_tt(110.0), Some(98.0));
        assert_eq!(recommended_packet_tt(120.0), Some(49.0));
        assert_eq!(recommended_packet_tt(200.0), Some(9.80));
        assert_eq!(recommended_packet_tt(100.0), None);
        assert_eq!(recommended_packet_tt(f64::NAN), None);
    }
}
