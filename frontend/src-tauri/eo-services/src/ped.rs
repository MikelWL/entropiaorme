//! The PED currency newtype (Project Entropia Dollar).
//!
//! A unit wrapper over `f64` so cost, loot, and ledger arithmetic
//! cannot silently mix with the dimensionless floats around it
//! (damage, multipliers, ratios). The arithmetic surface is deliberately
//! narrow: amounts add and subtract, an amount scales by a bare count
//! or factor, and dividing two amounts yields a dimensionless `f64`
//! (a multiplier or rate), which is exactly how the quantity behaves.
//! Serialisation is transparent, so a `Ped` field is byte-identical on
//! the wire to the `f64` it wraps.

use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Sub};

/// A PED amount. The inner value is exposed: the type exists to name
/// the unit at compile time, not to hide the number.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct Ped(pub f64);

impl Ped {
    pub const ZERO: Ped = Ped(0.0);

    /// The wrapped value, for rendering and persistence seams.
    pub fn value(self) -> f64 {
        self.0
    }

    /// Half-even rounding at the given decimal places, matching the
    /// wire normaliser the readouts render through.
    pub fn round_half_even(self, places: i32) -> Ped {
        Ped(eo_wire::normalizer::round_half_even(
            self.0,
            places as usize,
        ))
    }

    pub fn is_positive(self) -> bool {
        self.0 > 0.0
    }

    /// The magnitude of an amount is an amount (a reward-matching
    /// difference, a signed correction's size).
    pub fn abs(self) -> Ped {
        Ped(self.0.abs())
    }
}

impl Add for Ped {
    type Output = Ped;
    fn add(self, rhs: Ped) -> Ped {
        Ped(self.0 + rhs.0)
    }
}

impl AddAssign for Ped {
    fn add_assign(&mut self, rhs: Ped) {
        self.0 += rhs.0;
    }
}

impl Sub for Ped {
    type Output = Ped;
    fn sub(self, rhs: Ped) -> Ped {
        Ped(self.0 - rhs.0)
    }
}

/// Scaling an amount by a bare count or factor stays an amount.
impl Mul<f64> for Ped {
    type Output = Ped;
    fn mul(self, rhs: f64) -> Ped {
        Ped(self.0 * rhs)
    }
}

impl Mul<i64> for Ped {
    type Output = Ped;
    fn mul(self, rhs: i64) -> Ped {
        Ped(self.0 * rhs as f64)
    }
}

/// A ratio of two amounts is dimensionless (a multiplier or rate).
impl Div for Ped {
    type Output = f64;
    fn div(self, rhs: Ped) -> f64 {
        self.0 / rhs.0
    }
}

impl Sum for Ped {
    fn sum<I: Iterator<Item = Ped>>(iter: I) -> Ped {
        Ped(iter.map(|ped| ped.0).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_behaves_as_the_unit() {
        let cost = Ped(0.05) * 3_i64 + Ped(0.1);
        assert_eq!(cost, Ped(0.25));
        assert_eq!(Ped(5.0) / Ped(2.0), 2.5, "a ratio is dimensionless");
        assert_eq!(Ped(1.0) - Ped(0.25), Ped(0.75));
        assert_eq!([Ped(1.0), Ped(2.5)].into_iter().sum::<Ped>(), Ped(3.5));
        assert!(Ped(0.01).is_positive());
        assert!(!Ped::ZERO.is_positive());
        assert_eq!((Ped(1.0) - Ped(3.5)).abs(), Ped(2.5));
        assert_eq!(Ped(1.005).round_half_even(2), Ped(1.0));
    }

    #[test]
    fn serialises_transparently() {
        assert_eq!(serde_json::to_string(&Ped(4.5)).unwrap(), "4.5");
    }
}
