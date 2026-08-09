//! Weighted stock allocation and confirmed-sale arithmetic.
//!
//! Two questions live here, both pure. When a quantity leaves holdings,
//! which source activities produced it? And when a listing sells, how much
//! markup did it actually realise, and how much of that describes the
//! activity rather than the player's market execution?
//!
//! Fungible stock has no identity: 100 boards in inventory are not the
//! particular boards any one swing yielded. Asking the player to pick source
//! units, or imposing FIFO or LIFO, would invent a fact the game does not
//! record. The composition of what is held is the honest answer, so a sale
//! consumes every source in proportion to what that source still has open.
//!
//! A source is one provenance bucket: a harvest yield tier for Tree Cutting,
//! a mob species for Hunting. One item can hold both at once (Nanocubes
//! recycled from boards beside Nanocubes recycled from hides), and a sale of
//! it draws on each in proportion, crediting each activity its own share.
//!
//! Allocation is at (provenance, context, tool) granularity, not per source
//! event. Hunting keeps its session definition beside species so the same
//! immutable sale can be projected through either honest comparison axis.
//! Per-event granularity would fan a sale of a few hundred units into hundreds
//! of rows nothing reads, and the movement schema carries a nullable
//! `source_event_id` for it if that ever changes.

use crate::harvest_yield::HarvestYieldTier;

/// Sub-PED tolerance for treating a residual as closed. Quantities and TT
/// both round to hundredths at the display edge; anything under this is
/// float noise, not a real position.
const EPSILON: f64 = 1e-9;

/// Which activity a unit of stock traces back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockProvenance<'a> {
    /// Tree Cutting: the effective yield tier whose swings produced it.
    Harvest(HarvestYieldTier),
    /// Hunting: the mob species whose kills produced it.
    Hunt(&'a str),
}

/// One source's still-open position for a canonical item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourcePosition<'a> {
    /// `None` for stock whose provenance is genuinely unknown (migrated
    /// overlay rows, opening balances). Never a guess.
    pub provenance: Option<StockProvenance<'a>>,
    /// Hunting's user-designated context. `None` for harvesting, genuinely
    /// unassigned hunting, and movements recorded before this dimension.
    pub session_definition_id: Option<i64>,
    /// The tool that produced it. `None` when the swing recorded no tool,
    /// when the movement predates tool capture, or for hunted stock (a
    /// kill's loot is not a single tool's produce).
    pub tool_name: Option<&'a str>,
    pub quantity: f64,
}

/// One source's share of an outflow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceAllocation<'a> {
    pub provenance: Option<StockProvenance<'a>>,
    pub session_definition_id: Option<i64>,
    pub tool_name: Option<&'a str>,
    pub quantity: f64,
    pub tt_value: f64,
}

/// How a requested outflow divides across open positions.
#[derive(Debug, Clone, PartialEq)]
pub struct AllocationPlan<'a> {
    pub allocations: Vec<SourceAllocation<'a>>,
    /// Quantity covered by open positions carrying a known provenance.
    pub attributed_qty: f64,
    pub attributed_tt: f64,
    /// Quantity beyond tracked stock, plus any consumed from explicitly
    /// unattributed positions. Real value with no activity claim on it.
    pub unattributed_qty: f64,
    pub unattributed_tt: f64,
    /// The part of `unattributed_qty` that no open position covered at all,
    /// as opposed to the part drawn from positions of unknown provenance.
    ///
    /// The two are the same to the activity (neither may be claimed), but not
    /// to the stock ledger: an unattributed draw leaves a position that was
    /// there, while this leaves a position the app never recorded. Booking it
    /// as an outflow alone would drive the item's holdings negative, so the
    /// caller records the acquisition it implies before recording the outflow.
    pub excess_qty: f64,
    pub excess_tt: f64,
}

/// Split `quantity` of an item across its open source positions, weighted by
/// how much each source still holds.
///
/// A request larger than the total open position is not an error: the player
/// may hold stock from before tracking began, or from another source. The
/// excess is carried explicitly as unattributed rather than being spread over
/// the sources that happen to be tracked, which would credit them with output
/// they never produced.
///
/// `unit_tt` is the item's TT per unit, which Entropia fixes per item, so
/// quantity share and TT share are the same ratio.
pub fn allocate<'a>(
    positions: &[SourcePosition<'a>],
    quantity: f64,
    unit_tt: f64,
) -> AllocationPlan<'a> {
    let open: Vec<SourcePosition<'a>> = positions
        .iter()
        .copied()
        .filter(|position| position.quantity > EPSILON)
        .collect();
    let available: f64 = open.iter().map(|position| position.quantity).sum();

    let attributable = quantity.min(available).max(0.0);
    let excess = (quantity - attributable).max(0.0);

    let mut allocations = Vec::with_capacity(open.len());
    let mut assigned = 0.0_f64;
    for (index, position) in open.iter().enumerate() {
        // The last open position absorbs the residual so the parts sum to the
        // whole exactly, rather than drifting by a float ulp per source.
        let share = if index + 1 == open.len() {
            attributable - assigned
        } else {
            attributable * (position.quantity / available)
        };
        if share <= EPSILON {
            continue;
        }
        assigned += share;
        allocations.push(SourceAllocation {
            provenance: position.provenance,
            session_definition_id: position.session_definition_id,
            tool_name: position.tool_name,
            quantity: share,
            tt_value: share * unit_tt,
        });
    }

    // A position with no known provenance is consumed like any other (the
    // player really is holding it), but what it funds cannot be claimed by an
    // activity, so it lands on the unattributed side of the split.
    let attributed_qty: f64 = allocations
        .iter()
        .filter(|allocation| allocation.provenance.is_some())
        .map(|allocation| allocation.quantity)
        .sum();
    let unknown_qty: f64 = allocations
        .iter()
        .filter(|allocation| allocation.provenance.is_none())
        .map(|allocation| allocation.quantity)
        .sum();

    if excess > EPSILON {
        allocations.push(SourceAllocation {
            provenance: None,
            session_definition_id: None,
            tool_name: None,
            quantity: excess,
            tt_value: excess * unit_tt,
        });
    }

    let unattributed_qty = unknown_qty + excess;
    AllocationPlan {
        allocations,
        attributed_qty,
        attributed_tt: attributed_qty * unit_tt,
        unattributed_qty,
        unattributed_tt: unattributed_qty * unit_tt,
        excess_qty: excess,
        excess_tt: excess * unit_tt,
    }
}

/// What a confirmed sale realised, split into what the ledger records and
/// what the activity may claim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SaleOutcome {
    /// Sale proceeds above the whole listing's TT, and exactly what the
    /// ledger records as the gain.
    ///
    /// The TT itself is never income at sale time. For tracked stock it was
    /// already booked as loot when it dropped; for untracked stock the player
    /// still demonstrably held the value, so selling it converts a position
    /// rather than creating one. Netting off only the *tracked* TT would book
    /// the untracked units' principal as profit.
    pub gross_markup: f64,
    pub total_fees: f64,
    /// Gross markup after both auction fees.
    pub net_markup: f64,
    /// The fraction of the listing that tracked stock covered.
    pub attributed_share: f64,
    /// Net realised markup the activity may claim, to be divided across its
    /// contributing sources. The remainder of `net_markup` is the untracked
    /// share: real money, but with no activity able to claim it.
    pub activity_net_markup: f64,
}

/// Resolve a confirmed sale.
///
/// `tt_value` is the whole listing's TT and `attributed_tt` the part of it
/// covered by tracked stock. Fees are what the game actually charged: the
/// starting-bid fee taken at listing, and the additional fee taken at the
/// point of sale when an item sells above its starting bid.
///
/// Fees reduce the activity's realised markup because they are the direct
/// cost of capturing it, and the point-of-sale fee scales with how far above
/// TT the output sold. A fee on a listing that *expired* is a different
/// animal and never reaches here: the stock came back, so it describes market
/// execution, not the gameplay.
pub fn resolve_sale(
    tt_value: f64,
    attributed_tt: f64,
    final_price: f64,
    listing_fee: f64,
    sale_fee: f64,
) -> SaleOutcome {
    let gross_markup = final_price - tt_value;
    let total_fees = listing_fee + sale_fee;
    let net_markup = gross_markup - total_fees;
    let attributed_share = if tt_value.abs() > EPSILON {
        (attributed_tt / tt_value).clamp(0.0, 1.0)
    } else {
        0.0
    };
    SaleOutcome {
        gross_markup,
        total_fees,
        net_markup,
        attributed_share,
        activity_net_markup: net_markup * attributed_share,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(tier: HarvestYieldTier, quantity: f64) -> SourcePosition<'static> {
        SourcePosition {
            provenance: Some(StockProvenance::Harvest(tier)),
            session_definition_id: None,
            tool_name: None,
            quantity,
        }
    }

    fn species(name: &str, quantity: f64) -> SourcePosition<'_> {
        SourcePosition {
            provenance: Some(StockProvenance::Hunt(name)),
            session_definition_id: None,
            tool_name: None,
            quantity,
        }
    }

    fn tier_with_tool(tier: HarvestYieldTier, tool: &str, quantity: f64) -> SourcePosition<'_> {
        SourcePosition {
            provenance: Some(StockProvenance::Harvest(tier)),
            session_definition_id: None,
            tool_name: Some(tool),
            quantity,
        }
    }

    fn total_quantity(plan: &AllocationPlan) -> f64 {
        plan.allocations
            .iter()
            .map(|allocation| allocation.quantity)
            .sum()
    }

    /// The whole requested quantity is always accounted for, whether or not
    /// tracked stock covers it.
    #[test]
    fn allocation_conserves_the_requested_quantity() {
        let positions = [
            tier(HarvestYieldTier::Short, 60.0),
            tier(HarvestYieldTier::Long, 40.0),
        ];
        for requested in [1.0, 33.0, 100.0, 250.0] {
            let plan = allocate(&positions, requested, 0.03);
            assert!(
                (total_quantity(&plan) - requested).abs() < 1e-6,
                "requested {requested} did not conserve"
            );
            assert!(
                (plan.attributed_qty + plan.unattributed_qty - requested).abs() < 1e-6,
                "requested {requested} split does not sum back"
            );
        }
    }

    /// Sources are consumed in proportion to what each still holds.
    #[test]
    fn allocation_is_weighted_by_open_position() {
        let positions = [
            tier(HarvestYieldTier::Short, 75.0),
            tier(HarvestYieldTier::Long, 25.0),
        ];
        let plan = allocate(&positions, 40.0, 0.01);

        let short = plan
            .allocations
            .iter()
            .find(|allocation| {
                allocation.provenance == Some(StockProvenance::Harvest(HarvestYieldTier::Short))
            })
            .expect("short tier allocated");
        let long = plan
            .allocations
            .iter()
            .find(|allocation| {
                allocation.provenance == Some(StockProvenance::Harvest(HarvestYieldTier::Long))
            })
            .expect("long tier allocated");

        assert!((short.quantity - 30.0).abs() < 1e-9);
        assert!((long.quantity - 10.0).abs() < 1e-9);
        assert!((short.tt_value - 0.30).abs() < 1e-9);
        assert!((plan.unattributed_qty).abs() < 1e-9);
    }

    /// Hunted stock attributes exactly like harvested stock: a species
    /// position is a first-class source, not a lesser copy of a tier.
    #[test]
    fn species_positions_attribute_like_tiers() {
        let positions = [species("Carabok", 80.0), species("Berycled", 20.0)];
        let plan = allocate(&positions, 50.0, 0.02);

        let carabok = plan
            .allocations
            .iter()
            .find(|allocation| allocation.provenance == Some(StockProvenance::Hunt("Carabok")))
            .expect("carabok allocated");
        let berycled = plan
            .allocations
            .iter()
            .find(|allocation| allocation.provenance == Some(StockProvenance::Hunt("Berycled")))
            .expect("berycled allocated");

        assert!((carabok.quantity - 40.0).abs() < 1e-9);
        assert!((berycled.quantity - 10.0).abs() < 1e-9);
        assert!((plan.attributed_qty - 50.0).abs() < 1e-9);
        assert!((plan.unattributed_qty).abs() < 1e-9);
    }

    /// Session definition is an orthogonal provenance dimension: stock from
    /// two routines that hunted the same species stays separately claimable.
    #[test]
    fn definitions_inside_one_species_are_allocated_separately() {
        let positions = [
            SourcePosition {
                provenance: Some(StockProvenance::Hunt("Atrox")),
                session_definition_id: Some(7),
                tool_name: None,
                quantity: 75.0,
            },
            SourcePosition {
                provenance: Some(StockProvenance::Hunt("Atrox")),
                session_definition_id: Some(9),
                tool_name: None,
                quantity: 25.0,
            },
        ];
        let plan = allocate(&positions, 40.0, 0.01);

        let first = plan
            .allocations
            .iter()
            .find(|allocation| allocation.session_definition_id == Some(7))
            .expect("first definition allocated");
        let second = plan
            .allocations
            .iter()
            .find(|allocation| allocation.session_definition_id == Some(9))
            .expect("second definition allocated");
        assert!((first.quantity - 30.0).abs() < 1e-9);
        assert!((second.quantity - 10.0).abs() < 1e-9);
        assert_eq!(first.provenance, Some(StockProvenance::Hunt("Atrox")));
        assert_eq!(second.provenance, Some(StockProvenance::Hunt("Atrox")));
    }

    /// An item holding both harvest and hunt provenance (Nanocubes recycled
    /// from boards beside Nanocubes recycled from hides) draws on each in
    /// proportion, so a joint pile credits each activity its own share.
    #[test]
    fn mixed_provenance_draws_on_both_activities() {
        let positions = [
            tier(HarvestYieldTier::Long, 300.0),
            species("Carabok", 100.0),
        ];
        let plan = allocate(&positions, 200.0, 0.01);

        let harvest_share: f64 = plan
            .allocations
            .iter()
            .filter(|allocation| matches!(allocation.provenance, Some(StockProvenance::Harvest(_))))
            .map(|allocation| allocation.quantity)
            .sum();
        let hunt_share: f64 = plan
            .allocations
            .iter()
            .filter(|allocation| matches!(allocation.provenance, Some(StockProvenance::Hunt(_))))
            .map(|allocation| allocation.quantity)
            .sum();

        assert!((harvest_share - 150.0).abs() < 1e-9);
        assert!((hunt_share - 50.0).abs() < 1e-9);
        assert!((plan.attributed_qty - 200.0).abs() < 1e-9);
    }

    /// Selling more than is held keeps the tracked part attributed and names
    /// the rest unattributed, instead of inflating the tracked sources.
    #[test]
    fn excess_beyond_stock_is_explicitly_unattributed() {
        let positions = [tier(HarvestYieldTier::Huge, 10.0)];
        let plan = allocate(&positions, 25.0, 0.06);

        assert!((plan.attributed_qty - 10.0).abs() < 1e-9);
        assert!((plan.unattributed_qty - 15.0).abs() < 1e-9);
        assert!((plan.attributed_tt - 0.60).abs() < 1e-9);
        assert!((plan.unattributed_tt - 0.90).abs() < 1e-9);
        assert_eq!(
            plan.allocations
                .iter()
                .filter(|allocation| allocation.provenance.is_none())
                .count(),
            1,
            "the excess is one explicit unattributed row"
        );
    }

    /// Stock drawn from an unattributed position is unattributed but not
    /// excess: the position existed. Only quantity no position covered is
    /// excess, and the caller needs the two apart to keep holdings off
    /// negative.
    #[test]
    fn excess_is_only_the_part_no_position_covered() {
        let positions = [
            tier(HarvestYieldTier::Short, 10.0),
            SourcePosition {
                provenance: None,
                session_definition_id: None,
                tool_name: None,
                quantity: 5.0,
            },
        ];
        let plan = allocate(&positions, 20.0, 0.03);

        assert!((plan.attributed_qty - 10.0).abs() < 1e-9);
        // 5 drawn from the unattributed position, 5 covered by nothing at all.
        assert!((plan.unattributed_qty - 10.0).abs() < 1e-9);
        assert!((plan.excess_qty - 5.0).abs() < 1e-9);
        assert!((plan.excess_tt - 0.15).abs() < 1e-9);

        // A sale within tracked stock has no excess to make good.
        let within = allocate(&positions, 12.0, 0.03);
        assert!(within.excess_qty.abs() < 1e-9);
        assert!(
            within.unattributed_qty > 0.0,
            "still draws on the unattributed position"
        );
    }

    /// With nothing tracked at all, the whole sale is unattributed rather
    /// than silently credited somewhere.
    #[test]
    fn selling_with_no_tracked_stock_attributes_nothing() {
        let plan = allocate(&[], 12.0, 0.03);
        assert!((plan.attributed_qty).abs() < 1e-9);
        assert!((plan.unattributed_qty - 12.0).abs() < 1e-9);
    }

    /// Stock migrated without provenance is consumed, but funds no activity.
    #[test]
    fn unattributed_positions_consume_but_do_not_attribute() {
        let positions = [
            tier(HarvestYieldTier::Short, 50.0),
            SourcePosition {
                provenance: None,
                session_definition_id: None,
                tool_name: None,
                quantity: 50.0,
            },
        ];
        let plan = allocate(&positions, 100.0, 0.01);

        assert!((plan.attributed_qty - 50.0).abs() < 1e-9);
        assert!((plan.unattributed_qty - 50.0).abs() < 1e-9);
        assert!((total_quantity(&plan) - 100.0).abs() < 1e-9);
    }

    /// Two tools working the same tier are separate positions, so a sale
    /// draws on each in proportion to what it still holds rather than
    /// crediting the tier and leaving the tools uncredited.
    #[test]
    fn tools_inside_one_tier_are_allocated_separately() {
        let positions = [
            tier_with_tool(HarvestYieldTier::Long, "PH-3", 30.0),
            tier_with_tool(HarvestYieldTier::Long, "PH-4", 10.0),
        ];
        let plan = allocate(&positions, 20.0, 0.03);

        assert_eq!(plan.allocations.len(), 2);
        let ph3 = plan
            .allocations
            .iter()
            .find(|a| a.tool_name == Some("PH-3"))
            .expect("PH-3 allocated");
        let ph4 = plan
            .allocations
            .iter()
            .find(|a| a.tool_name == Some("PH-4"))
            .expect("PH-4 allocated");
        assert!((ph3.quantity - 15.0).abs() < 1e-9);
        assert!((ph4.quantity - 5.0).abs() < 1e-9);
        // Both still belong to the tier that owns them.
        assert_eq!(
            ph3.provenance,
            Some(StockProvenance::Harvest(HarvestYieldTier::Long))
        );
        assert_eq!(
            ph4.provenance,
            Some(StockProvenance::Harvest(HarvestYieldTier::Long))
        );
        assert!((plan.attributed_qty - 20.0).abs() < 1e-9);
    }

    /// Exhausted sources do not produce zero-quantity allocation rows.
    #[test]
    fn closed_positions_are_skipped() {
        let positions = [
            tier(HarvestYieldTier::Short, 0.0),
            tier(HarvestYieldTier::Long, 20.0),
        ];
        let plan = allocate(&positions, 5.0, 0.03);
        assert_eq!(plan.allocations.len(), 1);
        assert_eq!(
            plan.allocations[0].provenance,
            Some(StockProvenance::Harvest(HarvestYieldTier::Long))
        );
    }

    /// A fully tracked sale gives the activity the whole net markup, and the
    /// ledger only the money that was not already booked as loot TT.
    #[test]
    fn fully_tracked_sale_attributes_all_net_markup() {
        let outcome = resolve_sale(3.00, 3.00, 5.00, 0.50, 0.20);

        assert!((outcome.gross_markup - 2.00).abs() < 1e-9);
        assert!((outcome.net_markup - 1.30).abs() < 1e-9);
        assert!((outcome.attributed_share - 1.0).abs() < 1e-9);
        assert!((outcome.activity_net_markup - 1.30).abs() < 1e-9);
        // The 3.00 TT counted as loot when it dropped; only the uplift is new.
        assert!((outcome.gross_markup - 2.00).abs() < 1e-9);
    }

    /// A partly tracked sale gives the activity only its share, and the
    /// remainder reconciles exactly against the global figure.
    #[test]
    fn partly_tracked_sale_reconciles_against_the_ledger() {
        let outcome = resolve_sale(4.50, 3.00, 7.50, 0.50, 0.20);

        assert!((outcome.attributed_share - 2.0 / 3.0).abs() < 1e-9);
        assert!((outcome.net_markup - 2.30).abs() < 1e-9);
        assert!((outcome.activity_net_markup - 2.30 * 2.0 / 3.0).abs() < 1e-9);
        // The ledger records markup only. The untracked units' 1.50 of TT was
        // value the player already held; selling it converts a position and
        // must not read as profit.
        assert!((outcome.gross_markup - 3.00).abs() < 1e-9);

        // The activity claims its share; the remainder is untracked markup,
        // real money that no activity may claim.
        let global_only = outcome.net_markup - outcome.activity_net_markup;
        assert!((global_only - 2.30 / 3.0).abs() < 1e-9);
    }

    /// Selling below TT is a real loss and must stay negative, not clamp.
    #[test]
    fn a_sale_under_tt_realises_a_negative_markup() {
        let outcome = resolve_sale(3.00, 3.00, 2.50, 0.50, 0.0);
        assert!((outcome.gross_markup + 0.50).abs() < 1e-9);
        assert!((outcome.net_markup + 1.00).abs() < 1e-9);
        assert!(outcome.activity_net_markup < 0.0);
    }

    /// Nothing tracked means the activity claims nothing, however large the
    /// sale, while the ledger still records the full proceeds.
    #[test]
    fn untracked_sale_claims_nothing_for_the_activity() {
        let outcome = resolve_sale(4.00, 0.0, 9.00, 0.50, 0.30);
        assert!((outcome.attributed_share).abs() < 1e-9);
        assert!((outcome.activity_net_markup).abs() < 1e-9);
        // Markup only, even with nothing tracked: the 4.00 of TT sold was
        // still value the player held.
        assert!((outcome.gross_markup - 5.00).abs() < 1e-9);
    }

    /// A zero-TT listing cannot divide by its TT; it attributes nothing
    /// rather than producing a non-finite share.
    #[test]
    fn zero_tt_listing_does_not_divide_by_zero() {
        let outcome = resolve_sale(0.0, 0.0, 1.00, 0.50, 0.0);
        assert!(outcome.attributed_share.is_finite());
        assert!((outcome.attributed_share).abs() < 1e-9);
        assert!((outcome.activity_net_markup).abs() < 1e-9);
    }
}
