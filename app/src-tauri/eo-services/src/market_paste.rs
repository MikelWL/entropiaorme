//! Parse the game's market-ledger clipboard export into per-item
//! markup readings.
//!
//! The auction house exports a tab-separated table (its "copy" and
//! "copy CSV" buttons emit the same bytes): a header row, then one row
//! per item carrying `Item, Tier` and five aggregation horizons of
//! `Markup, Sales` pairs (day, week, month, year, decade). Markup is a
//! percentage or `N/A` (no sales in that horizon); Sales is TT turnover,
//! represented as a number with an optional `K`/`M` multiplier and a
//! `PEC`/`PED` unit.
//!
//! Parsing follows the chat-log parser's discipline: pure functions
//! over lines, no panics, malformed input degrades per line rather
//! than failing the paste. Unlike the chat log, the caller here is a
//! review-before-accept flow, so skipped lines are reported with their
//! line number and a human-readable reason instead of vanishing.
//! Tolerances beyond the observed format, coded deliberately: runs of
//! two or more spaces accepted as the delimiter (clipboard transport
//! through chat or editors can mangle tabs), and a comma accepted as
//! the decimal separator (locale risk; the game's numbers carry no
//! thousands separators to collide with).

use std::sync::OnceLock;

use regex::Regex;

/// The five aggregation horizons of one export row, in column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarketHorizon {
    Day,
    Week,
    Month,
    Year,
    Decade,
}

impl MarketHorizon {
    /// Every horizon, in the export's column order (the order
    /// [`MarketPasteRow::readings`] is indexed by).
    pub const ALL: [MarketHorizon; 5] = [
        MarketHorizon::Day,
        MarketHorizon::Week,
        MarketHorizon::Month,
        MarketHorizon::Year,
        MarketHorizon::Decade,
    ];

    /// The stored vocabulary value (the `market_observations.horizon`
    /// column).
    pub fn as_str(self) -> &'static str {
        match self {
            MarketHorizon::Day => "day",
            MarketHorizon::Week => "week",
            MarketHorizon::Month => "month",
            MarketHorizon::Year => "year",
            MarketHorizon::Decade => "decade",
        }
    }

    /// Parse the stored vocabulary value back into the horizon.
    pub fn from_stored(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|horizon| horizon.as_str() == value)
    }
}

/// One horizon's reading: the markup percentage (`None` where the game
/// reported `N/A`, meaning no sales in that horizon) and the sales
/// volume normalised to PED.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarketReading {
    pub markup_pct: Option<f64>,
    pub sales_ped: f64,
}

/// One parsed item row: the item, its tier, and the five horizon
/// readings in [`MarketHorizon::ALL`] order.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketPasteRow {
    pub item_name: String,
    pub tier: i64,
    pub readings: [MarketReading; 5],
}

/// One line the parser could not use, reported for the review flow.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedLine {
    /// 1-based line number within the paste.
    pub line_number: usize,
    pub content: String,
    pub reason: String,
}

/// The parse outcome: the usable rows and the lines that were not
/// (blank lines and the header are dropped silently, not reported).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketPasteParse {
    pub rows: Vec<MarketPasteRow>,
    pub skipped: Vec<SkippedLine>,
}

/// The export's column count: item, tier, then five (markup, sales)
/// pairs.
const COLUMN_COUNT: usize = 12;

/// Parse a full market-ledger paste. Never fails: unusable lines land
/// in [`MarketPasteParse::skipped`] with a reason.
pub fn parse_market_paste(text: &str) -> MarketPasteParse {
    let mut parse = MarketPasteParse::default();
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_fields(line);
        if is_header(&fields) {
            continue;
        }
        match parse_row(&fields) {
            Ok(row) => parse.rows.push(row),
            Err(reason) => parse.skipped.push(SkippedLine {
                line_number: index + 1,
                content: line.to_string(),
                reason,
            }),
        }
    }
    parse
}

/// Split one line into cells: tabs when present, else runs of two or
/// more spaces (item names carry single spaces, so single spaces never
/// delimit).
fn split_fields(line: &str) -> Vec<&str> {
    static MULTI_SPACE: OnceLock<Regex> = OnceLock::new();
    if line.contains('\t') {
        return line.split('\t').map(str::trim).collect();
    }
    MULTI_SPACE
        .get_or_init(|| Regex::new(r"\s{2,}").expect("static pattern"))
        .split(line)
        .map(str::trim)
        .filter(|cell| !cell.is_empty())
        .collect()
}

/// The header row, recognised by its first two column titles.
fn is_header(fields: &[&str]) -> bool {
    fields.first().is_some_and(|cell| *cell == "Item")
        && fields.get(1).is_some_and(|cell| *cell == "Tier")
}

fn parse_row(fields: &[&str]) -> Result<MarketPasteRow, String> {
    if fields.len() != COLUMN_COUNT {
        return Err(format!(
            "expected {COLUMN_COUNT} columns, found {}",
            fields.len()
        ));
    }
    let item_name = fields[0].to_string();
    if item_name.is_empty() {
        return Err("empty item name".to_string());
    }
    let tier: i64 = fields[1]
        .parse()
        .map_err(|_| format!("unreadable tier {:?}", fields[1]))?;

    let mut readings = [MarketReading {
        markup_pct: None,
        sales_ped: 0.0,
    }; 5];
    for (slot, reading) in readings.iter_mut().enumerate() {
        let markup_cell = fields[2 + slot * 2];
        let sales_cell = fields[3 + slot * 2];
        *reading = MarketReading {
            markup_pct: parse_markup(markup_cell)
                .map_err(|_| format!("unreadable markup {markup_cell:?}"))?,
            sales_ped: parse_sales(sales_cell)
                .map_err(|_| format!("unreadable sales turnover {sales_cell:?}"))?,
        };
    }
    Ok(MarketPasteRow {
        item_name,
        tier,
        readings,
    })
}

/// A markup cell: `106.880%` (or a comma decimal) to its percentage,
/// `N/A` to `None`.
fn parse_markup(cell: &str) -> Result<Option<f64>, ()> {
    if cell.eq_ignore_ascii_case("N/A") {
        return Ok(None);
    }
    let number = cell.strip_suffix('%').ok_or(())?;
    parse_number(number).map(Some)
}

/// A sales cell: `13.500K PED`, `0.000 PEC` and kin, normalised to PED
/// (1 PEC = 0.01 PED).
fn parse_sales(cell: &str) -> Result<f64, ()> {
    static SALES: OnceLock<Regex> = OnceLock::new();
    let pattern = SALES.get_or_init(|| {
        Regex::new(r"^(?P<number>[0-9]+(?:[.,][0-9]+)?)(?P<scale>[KM]?)\s*(?P<unit>PED|PEC)$")
            .expect("static pattern")
    });
    let captures = pattern.captures(cell).ok_or(())?;
    let value = parse_number(&captures["number"])?;
    let scale = match &captures["scale"] {
        "K" => 1_000.0,
        "M" => 1_000_000.0,
        _ => 1.0,
    };
    let unit = match &captures["unit"] {
        "PEC" => 0.01,
        _ => 1.0,
    };
    Ok(value * scale * unit)
}

/// A plain decimal, dot or comma separated (the game emits no
/// thousands separators).
fn parse_number(text: &str) -> Result<f64, ()> {
    let normalised = text.replace(',', ".");
    if normalised.chars().filter(|c| *c == '.').count() > 1 {
        return Err(());
    }
    normalised.parse().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real export, verbatim (both the "copy" and "copy CSV" buttons
    /// emit this same tab-separated table).
    const SAMPLE: &str = "Item\tTier\tDay Markup\tDay Sales\tWeek Markup\tWeek Sales\tMonth Markup\tMonth Sales\tYear Markup\tYear Sales\tDecade Markup\tDecade Sales\n\
Carabok Leg Fur\t0\tN/A\t0.000 PEC\tN/A\t0.000 PEC\tN/A\t0.000 PEC\t109.380%\t6.400 PED\t339.100%\t375.400 PED\n\
Carabok Hide\t0\t106.880%\t451.900 PED\t107.160%\t531.900 PED\t106.020%\t979.040 PED\t108.280%\t13.500K PED\t158.920%\t35.300K PED\n\
Animal Oil Residue\t0\t104.000%\t100.000 PED\t100.540%\t6.400K PED\t100.570%\t23.400K PED\t100.680%\t367.900K PED\t101.020%\t6.400M PED\n\
Animal Muscle Oil\t0\t101.900%\t13.400K PED\t102.100%\t50.500K PED\t102.240%\t204.800K PED\t102.960%\t2.800M PED\t103.190%\t45.300M PED\n\
Advanced Stone Extractor\t0\tN/A\t0.000 PEC\t116.190%\t54.220 PED\t115.940%\t817.660 PED\t112.870%\t7.500K PED\t114.300%\t85.700K PED\n\
Basic Cloth Extractor\t0\tN/A\t0.000 PEC\t110.000%\t10.000 PED\t114.260%\t67.390 PED\t133.730%\t1.700K PED\t135.980%\t8.100K PED\n\
Inferior Cloth Extractor\t0\tN/A\t0.000 PEC\t140.000%\t15.000 PED\t115.250%\t67.680 PED\t108.210%\t2.700K PED\t171.880%\t40.600K PED\n";

    #[test]
    fn parses_the_real_sample_verbatim() {
        let parse = parse_market_paste(SAMPLE);
        assert!(parse.skipped.is_empty(), "skipped: {:?}", parse.skipped);
        assert_eq!(parse.rows.len(), 7);

        // N/A markups decode to None with the zero PEC volume in PED.
        let fur = &parse.rows[0];
        assert_eq!(fur.item_name, "Carabok Leg Fur");
        assert_eq!(fur.tier, 0);
        assert_eq!(fur.readings[0].markup_pct, None);
        assert_eq!(fur.readings[0].sales_ped, 0.0);
        assert_eq!(fur.readings[3].markup_pct, Some(109.380));
        assert_eq!(fur.readings[3].sales_ped, 6.4);
        assert_eq!(fur.readings[4].markup_pct, Some(339.100));
        assert_eq!(fur.readings[4].sales_ped, 375.4);

        // K and M volume scales normalise to plain PED.
        let hide = &parse.rows[1];
        assert_eq!(hide.readings[3].sales_ped, 13_500.0);
        let oil = &parse.rows[3];
        assert_eq!(oil.item_name, "Animal Muscle Oil");
        assert_eq!(oil.readings[4].markup_pct, Some(103.190));
        assert_eq!(oil.readings[4].sales_ped, 45_300_000.0);
    }

    #[test]
    fn space_runs_delimit_when_transport_mangles_tabs() {
        let mangled = SAMPLE.replace('\t', "    ");
        let parse = parse_market_paste(&mangled);
        assert!(parse.skipped.is_empty(), "skipped: {:?}", parse.skipped);
        assert_eq!(parse.rows.len(), 7);
        // Single spaces inside names survive the space-run split.
        assert_eq!(parse.rows[4].item_name, "Advanced Stone Extractor");
        assert_eq!(parse.rows[4].readings[1].markup_pct, Some(116.190));
        assert_eq!(parse.rows[4].readings[1].sales_ped, 54.22);
    }

    #[test]
    fn comma_decimals_parse_as_the_locale_tolerance() {
        let line = "Carabok Hide\t0\t106,880%\t451,900 PED\t107.160%\t531.900 PED\t\
106.020%\t979.040 PED\t108.280%\t13,500K PED\t158.920%\t35.300K PED";
        let parse = parse_market_paste(line);
        assert_eq!(parse.rows.len(), 1);
        assert_eq!(parse.rows[0].readings[0].markup_pct, Some(106.880));
        assert_eq!(parse.rows[0].readings[0].sales_ped, 451.9);
        assert_eq!(parse.rows[0].readings[3].sales_ped, 13_500.0);
    }

    #[test]
    fn unusable_lines_report_their_reason_and_spare_the_rest() {
        let text = "Carabok Hide\t0\t106.880%\t451.900 PED\n\
Carabok Hide\t0\t106.880%\t451.900 PED\t107.160%\t531.900 PED\t106.020%\t979.040 PED\t\
108.280%\t13.500K PED\t158.920%\tsoon PED\n\
Carabok Hide\tzero\t106.880%\t451.900 PED\t107.160%\t531.900 PED\t106.020%\t979.040 PED\t\
108.280%\t13.500K PED\t158.920%\t35.300K PED\n\
Animal Oil Residue\t0\t104.000%\t100.000 PED\t100.540%\t6.400K PED\t100.570%\t23.400K PED\t\
100.680%\t367.900K PED\t101.020%\t6.400M PED";
        let parse = parse_market_paste(text);
        assert_eq!(parse.rows.len(), 1);
        assert_eq!(parse.rows[0].item_name, "Animal Oil Residue");
        assert_eq!(parse.skipped.len(), 3);
        assert_eq!(parse.skipped[0].line_number, 1);
        assert!(parse.skipped[0].reason.contains("expected 12 columns"));
        assert!(parse.skipped[1]
            .reason
            .contains("unreadable sales turnover"));
        assert!(parse.skipped[2].reason.contains("unreadable tier"));
    }

    #[test]
    fn blank_lines_and_headers_drop_silently() {
        let text = "\n  \nItem\tTier\tDay Markup\tDay Sales\tWeek Markup\tWeek Sales\t\
Month Markup\tMonth Sales\tYear Markup\tYear Sales\tDecade Markup\tDecade Sales\n\n";
        let parse = parse_market_paste(text);
        assert!(parse.rows.is_empty());
        assert!(parse.skipped.is_empty());
    }

    #[test]
    fn a_data_row_named_item_is_not_mistaken_for_the_header() {
        // The header is recognised by BOTH "Item" and "Tier" in the first
        // two columns; a real 12-column row whose item is literally
        // "Item" (tier 0) must parse as data, not be dropped as a header.
        let line = "Item\t0\t106.880%\t451.900 PED\t107.160%\t531.900 PED\t\
106.020%\t979.040 PED\t108.280%\t13.500K PED\t158.920%\t35.300K PED";
        let parse = parse_market_paste(line);
        assert_eq!(parse.rows.len(), 1);
        assert_eq!(parse.rows[0].item_name, "Item");
    }

    #[test]
    fn sales_apply_both_the_scale_and_the_unit() {
        // K/M scale (x1000/x1_000_000) and the PEC unit (1/100 PED) are
        // independent multipliers that must both land: a value carrying
        // both distinguishes a dropped unit arm or a divided factor.
        assert!((parse_sales("5.000K PEC").unwrap() - 50.0).abs() < 1e-9);
        assert!((parse_sales("5.000 PEC").unwrap() - 0.05).abs() < 1e-9);
        assert_eq!(parse_sales("5.000 PED"), Ok(5.0));
        assert_eq!(parse_sales("2.000M PED"), Ok(2_000_000.0));
    }

    #[test]
    fn numbers_accept_integers_and_reject_multiple_separators() {
        // A separator-free integer must parse (the dot-count guard fires
        // only on two or more separators, never on zero).
        assert_eq!(parse_number("150"), Ok(150.0));
        assert!((parse_number("106,880").unwrap() - 106.880).abs() < 1e-9);
        assert!(parse_number("1.2.3").is_err());
    }

    #[test]
    fn horizon_vocabulary_round_trips() {
        for horizon in MarketHorizon::ALL {
            assert_eq!(MarketHorizon::from_stored(horizon.as_str()), Some(horizon));
        }
        assert_eq!(MarketHorizon::from_stored("fortnight"), None);
    }
}
