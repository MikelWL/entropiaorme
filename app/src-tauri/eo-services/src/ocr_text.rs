//! Reading numbers out of recognised text.
//!
//! The recogniser answers with whatever was in the rectangle, which for
//! a game panel means the figure plus its furniture: a currency suffix,
//! thin spaces between digit cells, a comma where a point belongs. The
//! rectangle cannot exclude the furniture either, because a longer value
//! extends into the space a shorter one's suffix occupied.
//!
//! So the number is taken as the first digit run with an optional
//! fraction, and everything after it is ignored. The repair-cost read
//! established that shape; the sale-window read needs it too, with one
//! difference: it must be able to refuse. A cost that fails to parse can
//! show as zero, but a listing field that fails to parse must stay empty
//! so nobody commits a figure the screen never showed.

use crate::skill_panel::digit_value;

/// The first digit run in `text` as a number, or None when there is no
/// digit to read. Commas read as decimal points and spaces drop, so a
/// digit-celled figure ("0 0 0 9 2 7 6") and a comma decimal ("12,50")
/// both parse. The digit class is Unicode-wide by way of `digit_value`,
/// which covers the recogniser's ASCII and fullwidth alphabet.
pub fn parse_amount(text: &str) -> Option<f64> {
    let cleaned: String = text.replace(',', ".").replace(' ', "");
    let chars: Vec<char> = cleaned.chars().collect();
    let start = chars.iter().position(|ch| digit_value(*ch).is_some())?;
    let mut number = String::new();
    let mut seen_dot = false;
    for &ch in &chars[start..] {
        if let Some(value) = digit_value(ch) {
            number.push(char::from_digit(value, 10).expect("decimal digit"));
        } else if ch == '.' && !seen_dot {
            // The optional fraction: one dot, then digits only.
            seen_dot = true;
            number.push(ch);
        } else {
            break;
        }
    }
    number.parse().ok()
}

/// The repair terminal's cost, where an unreadable figure is zero: the
/// cost is shown beside its own raw text and confidence, so a zero reads
/// as "nothing was found" in context rather than as a free repair.
pub fn parse_cost(text: &str) -> f64 {
    parse_amount(text).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_parse_through_comma_space_and_noise() {
        assert_eq!(parse_cost("12.50"), 12.5);
        assert_eq!(parse_cost("12,50"), 12.5);
        assert_eq!(parse_cost("1 234,5"), 1234.5);
        assert_eq!(parse_cost("PED 0.05"), 0.05);
        assert_eq!(parse_cost("cost: 2.20 PED"), 2.2);
        assert_eq!(parse_cost("12."), 12.0);
        assert_eq!(parse_cost("1.2.3"), 1.2);
        assert_eq!(parse_cost("no digits"), 0.0);
        assert_eq!(parse_cost(""), 0.0);
        assert_eq!(parse_cost(".5"), 5.0);
        assert_eq!(parse_cost("0,0,7"), 0.0);
        // Fullwidth digits convert by value; a fullwidth decimal point is
        // not a fraction dot, so the run ends there.
        assert_eq!(parse_cost("\u{ff11}\u{ff12}.50"), 12.5);
        assert_eq!(parse_cost("1\u{ff12}3"), 123.0);
        assert_eq!(parse_cost("\u{ff11}\u{ff12}\u{ff0e}50"), 12.0);
    }

    #[test]
    fn an_amount_with_no_digits_refuses_rather_than_reading_zero() {
        assert_eq!(parse_amount("no digits"), None);
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("PED"), None);
        // The distinction the refusal buys: a real zero is still a zero.
        assert_eq!(parse_amount("0.00 PED"), Some(0.0));
        assert_eq!(parse_amount("0"), Some(0.0));
    }

    #[test]
    fn the_sale_windows_own_shapes_parse() {
        // A currency suffix shares the rectangle with the value, because
        // a longer value would otherwise run into where it sits.
        assert_eq!(parse_amount("275.48 PED"), Some(275.48));
        assert_eq!(parse_amount("64.51 PED"), Some(64.51));
        assert_eq!(parse_amount("12275.48 PED"), Some(12275.48));
        // Bid fields render one digit per cell, so the recogniser may
        // return them spaced, and they carry leading zeros.
        assert_eq!(parse_amount("0 0 0 9 2 7 6"), Some(9276.0));
        assert_eq!(parse_amount("0009276"), Some(9276.0));
        assert_eq!(parse_amount("0000000"), Some(0.0));
        // Quantity and duration are plain integers.
        assert_eq!(parse_amount("2754889"), Some(2754889.0));
        assert_eq!(parse_amount("6"), Some(6.0));
    }
}
