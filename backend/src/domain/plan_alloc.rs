use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PlanNodeInput {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub target_pct: Decimal,
    pub tolerance_band_pct: Option<Decimal>,
    pub bind_kind: String,
    pub category_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub sort_order: i64,
    pub color: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanNodeAllocation {
    pub id: i64,
    pub name: String,
    pub bind_kind: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub tolerance_band_pct: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub drift_pct: Decimal,
    pub out_of_band: bool,
    #[serde(with = "rust_decimal::serde::str")]
    pub rebalance_idr: Decimal,
    pub color: Option<String>,
    pub children: Vec<PlanNodeAllocation>,
}

/// Compute the recursive allocation tree.
///
/// - instrument leaf => its market value.
/// - group node      => sum of children.
/// - category node   => total IDR of all instruments in that category; explicit
///   children break it down and the unclaimed remainder surfaces as a synthetic
///   "Lainnya" child.
/// Percentages and drift are computed RELATIVE TO THE PARENT (root parent = total).
pub fn compute_plan_tree(
    nodes: &[PlanNodeInput],
    instrument_value: &HashMap<i64, Decimal>,
    instrument_category: &HashMap<i64, Option<i64>>,
    total_idr: Decimal,
) -> Vec<PlanNodeAllocation> {
    // children index
    let mut children: HashMap<i64, Vec<&PlanNodeInput>> = HashMap::new();
    let mut roots: Vec<&PlanNodeInput> = Vec::new();
    for node in nodes {
        match node.parent_id {
            Some(p) => children.entry(p).or_default().push(node),
            None => roots.push(node),
        }
    }
    let ctx = Ctx { children, instrument_value, instrument_category };

    let mut out: Vec<PlanNodeAllocation> = roots
        .iter()
        .map(|r| build(r, total_idr, total_idr, &ctx))
        .collect();
    // Root-level "Lainnya": everything not covered by a root node.
    let claimed: Decimal = out.iter().map(|x| x.actual_value_idr).sum();
    let remainder = total_idr - claimed;
    if remainder > Decimal::ZERO {
        out.push(lainnya(-1, remainder, total_idr));
    }
    out
}

struct Ctx<'a> {
    children: HashMap<i64, Vec<&'a PlanNodeInput>>,
    instrument_value: &'a HashMap<i64, Decimal>,
    instrument_category: &'a HashMap<i64, Option<i64>>,
}

fn category_total(cat_id: i64, ctx: &Ctx) -> Decimal {
    ctx.instrument_value
        .iter()
        .filter(|(iid, _)| ctx.instrument_category.get(iid).copied().flatten() == Some(cat_id))
        .map(|(_, v)| *v)
        .sum()
}

fn actual_value(node: &PlanNodeInput, ctx: &Ctx) -> Decimal {
    match node.bind_kind.as_str() {
        "instrument" => node
            .instrument_id
            .and_then(|iid| ctx.instrument_value.get(&iid).copied())
            .unwrap_or(Decimal::ZERO),
        "category" => node.category_id.map(|c| category_total(c, ctx)).unwrap_or(Decimal::ZERO),
        _ /* group */ => sorted_children(node, ctx).iter().map(|c| actual_value(c, ctx)).sum(),
    }
}

fn sorted_children<'a>(node: &PlanNodeInput, ctx: &Ctx<'a>) -> Vec<&'a PlanNodeInput> {
    let mut kids = ctx.children.get(&node.id).cloned().unwrap_or_default();
    kids.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.id.cmp(&b.id)));
    kids
}

fn build(node: &PlanNodeInput, parent_actual: Decimal, parent_target: Decimal, ctx: &Ctx) -> PlanNodeAllocation {
    let hundred = Decimal::from(100);
    let actual = actual_value(node, ctx);
    let target_value = parent_target * node.target_pct / hundred;
    let actual_pct = if parent_actual.is_zero() { Decimal::ZERO } else { actual / parent_actual * hundred };
    let drift = actual_pct - node.target_pct;
    let out_of_band = match node.tolerance_band_pct {
        Some(band) => drift.abs() > band,
        None => false,
    };

    let mut children: Vec<PlanNodeAllocation> = sorted_children(node, ctx)
        .iter()
        .map(|c| build(c, actual, target_value, ctx))
        .collect();

    // Category nodes surface their unbroken-down remainder as "Lainnya".
    if node.bind_kind == "category" {
        let claimed: Decimal = children.iter().map(|x| x.actual_value_idr).sum();
        let remainder = actual - claimed;
        if remainder > Decimal::ZERO {
            let syn_id = -2 - node.category_id.unwrap_or(0);
            children.push(lainnya(syn_id, remainder, actual));
        }
    }

    PlanNodeAllocation {
        id: node.id,
        name: node.name.clone(),
        bind_kind: node.bind_kind.clone(),
        target_pct: node.target_pct,
        tolerance_band_pct: node.tolerance_band_pct,
        actual_pct,
        actual_value_idr: actual,
        target_value_idr: target_value,
        drift_pct: drift,
        out_of_band,
        rebalance_idr: target_value - actual,
        color: node.color.clone(),
        children,
    }
}

/// A synthetic, target-less remainder node. Never flags out-of-band.
fn lainnya(id: i64, value: Decimal, parent_actual: Decimal) -> PlanNodeAllocation {
    let hundred = Decimal::from(100);
    let actual_pct = if parent_actual.is_zero() { Decimal::ZERO } else { value / parent_actual * hundred };
    PlanNodeAllocation {
        id,
        name: "Lainnya".to_string(),
        bind_kind: "lainnya".to_string(),
        target_pct: Decimal::ZERO,
        tolerance_band_pct: None,
        actual_pct,
        actual_value_idr: value,
        target_value_idr: Decimal::ZERO,
        drift_pct: actual_pct,
        out_of_band: false,
        rebalance_idr: Decimal::ZERO,
        color: None,
        children: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn n(id: i64, parent: Option<i64>, name: &str, target: Decimal, tol: Option<Decimal>, kind: &str, cat: Option<i64>, ins: Option<i64>) -> PlanNodeInput {
        PlanNodeInput {
            id, parent_id: parent, name: name.into(), target_pct: target,
            tolerance_band_pct: tol, bind_kind: kind.into(),
            category_id: cat, instrument_id: ins, sort_order: 0, color: None,
        }
    }

    #[test]
    fn category_root_with_instrument_child_and_lainnya() {
        // Portfolio = 100 IDR. Category "Saham" (id=1) total = 20 (BBCA 12 + BBRI 8).
        // Uncategorized = 80.
        let nodes = vec![
            n(1, None, "Saham", dec!(30), Some(dec!(5)), "category", Some(1), None),
            n(2, Some(1), "BBCA", dec!(40), None, "instrument", None, Some(10)),
        ];
        let mut iv = std::collections::HashMap::new();
        iv.insert(10, dec!(12)); // BBCA
        iv.insert(11, dec!(8));  // BBRI (in Saham, not broken out)
        iv.insert(99, dec!(80)); // uncategorized
        let mut ic = std::collections::HashMap::new();
        ic.insert(10, Some(1));
        ic.insert(11, Some(1));
        ic.insert(99, None);

        let tree = compute_plan_tree(&nodes, &iv, &ic, dec!(100));

        // Roots: Saham + synthetic root "Lainnya" (80).
        let saham = tree.iter().find(|x| x.id == 1).unwrap();
        assert_eq!(saham.actual_value_idr, dec!(20));
        assert_eq!(saham.actual_pct, dec!(20));      // 20/100
        assert_eq!(saham.target_value_idr, dec!(30)); // 30% of 100
        assert_eq!(saham.drift_pct, dec!(-10));       // 20 - 30
        assert!(saham.out_of_band);                   // |10| > 5
        assert_eq!(saham.rebalance_idr, dec!(10));    // 30 - 20

        // BBCA child: 12 of Saham's 20 = 60% (vs 40% target).
        let bbca = saham.children.iter().find(|x| x.id == 2).unwrap();
        assert_eq!(bbca.actual_value_idr, dec!(12));
        assert_eq!(bbca.actual_pct, dec!(60));
        assert_eq!(bbca.target_value_idr, dec!(12)); // 40% of Saham target 30
        assert_eq!(bbca.drift_pct, dec!(20));

        // Synthetic "Lainnya" under Saham: 20 - 12 = 8.
        let saham_lain = saham.children.iter().find(|x| x.actual_value_idr == dec!(8)).unwrap();
        assert_eq!(saham_lain.name, "Lainnya");
        assert_eq!(saham_lain.actual_pct, dec!(40)); // 8/20
        assert!(!saham_lain.out_of_band);

        // Root "Lainnya": 100 - 20 = 80.
        let root_lain = tree.iter().find(|x| x.id == -1).unwrap();
        assert_eq!(root_lain.actual_value_idr, dec!(80));
        assert_eq!(root_lain.actual_pct, dec!(80));
        assert!(!root_lain.out_of_band);

        // Whole tree reconciles to net worth.
        let root_total: Decimal = tree.iter().map(|x| x.actual_value_idr).sum();
        assert_eq!(root_total, dec!(100));
    }

    #[test]
    fn group_node_sums_children() {
        // Group "Equity" with two instrument children; no category binding.
        let nodes = vec![
            n(1, None, "Equity", dec!(100), None, "group", None, None),
            n(2, Some(1), "A", dec!(50), None, "instrument", None, Some(10)),
            n(3, Some(1), "B", dec!(50), None, "instrument", None, Some(11)),
        ];
        let mut iv = std::collections::HashMap::new();
        iv.insert(10, dec!(30));
        iv.insert(11, dec!(70));
        let ic = std::collections::HashMap::new(); // categories irrelevant for group/instrument
        let tree = compute_plan_tree(&nodes, &iv, &ic, dec!(100));
        let equity = &tree[0];
        assert_eq!(equity.actual_value_idr, dec!(100)); // 30 + 70
        // Group nodes get NO synthetic Lainnya (only category nodes do).
        assert_eq!(equity.children.len(), 2);
    }

    #[test]
    fn empty_portfolio_is_zero_not_panic() {
        let nodes = vec![n(1, None, "Saham", dec!(100), None, "category", Some(1), None)];
        let iv = std::collections::HashMap::new();
        let ic = std::collections::HashMap::new();
        let tree = compute_plan_tree(&nodes, &iv, &ic, dec!(0));
        assert_eq!(tree[0].actual_value_idr, dec!(0));
        assert_eq!(tree[0].actual_pct, dec!(0));
    }
}
