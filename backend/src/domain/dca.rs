use crate::domain::allocation::CategoryInput;
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DcaMode {
    Rebalance,
    Mixed,
    Proportional,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DcaPhase {
    Rebalance,
    Proportional,
    Mixed,
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct DcaCategoryLine {
    pub category_id: i64,
    pub name: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub current_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub drift_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub allocate_idr: Decimal,
    pub phase: DcaPhase,
}

#[derive(Debug, Clone, Serialize)]
pub struct DcaPlan {
    #[serde(with = "rust_decimal::serde::str")]
    pub budget_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_value_idr: Decimal,
    pub mode: DcaMode,
    pub lines: Vec<DcaCategoryLine>,
    pub note: Option<String>,
}

/// Per-category raw (unrounded) split into its two phase contributions.
#[derive(Debug, Clone, Copy)]
struct RawAlloc {
    rebalance: Decimal,
    proportional: Decimal,
}

fn raw_allocations(categories: &[CategoryInput], budget: Decimal) -> Vec<RawAlloc> {
    let hundred = Decimal::from(100);
    let total: Decimal = categories.iter().map(|c| c.value_idr).sum();
    let projected = total + budget; // T = V + B
    let n = categories.len();
    let mut out = vec![RawAlloc { rebalance: Decimal::ZERO, proportional: Decimal::ZERO }; n];

    // Phase 1: shortfalls for categories under target (by CURRENT %) beyond their band.
    let mut shortfalls = vec![Decimal::ZERO; n];
    for (i, c) in categories.iter().enumerate() {
        if c.target_pct <= Decimal::ZERO {
            continue;
        }
        let actual_pct = if total.is_zero() {
            Decimal::ZERO
        } else {
            c.value_idr / total * hundred
        };
        let band = c.tolerance_band_pct.unwrap_or(Decimal::ZERO);
        if actual_pct >= c.target_pct - band {
            continue; // within band or over target -> not a rebalance target
        }
        let target_value = projected * c.target_pct / hundred;
        let short = target_value - c.value_idr;
        if short > Decimal::ZERO {
            shortfalls[i] = short;
        }
    }
    let total_short: Decimal = shortfalls.iter().sum();

    if total_short >= budget && total_short > Decimal::ZERO {
        // Can't close every gap: split the whole budget proportional to gaps.
        for i in 0..n {
            if shortfalls[i] > Decimal::ZERO {
                out[i].rebalance = budget * shortfalls[i] / total_short;
            }
        }
        return out;
    }

    // Budget covers all gaps: fill each gap, then spread the remainder by target weight.
    for i in 0..n {
        out[i].rebalance = shortfalls[i];
    }
    let remainder = budget - total_short;
    if remainder > Decimal::ZERO {
        for (i, c) in categories.iter().enumerate() {
            if c.target_pct > Decimal::ZERO {
                // divide by 100 (not by sum of targets): if targets sum < 100, the slack stays cash.
                out[i].proportional = remainder * c.target_pct / hundred;
            }
        }
    }
    out
}

/// Largest-remainder rounding so each line is a multiple of `step` and the
/// total never exceeds the intended sum. `step <= 0` disables rounding.
fn apply_rounding(raws: &[Decimal], step: Decimal) -> Vec<Decimal> {
    if step <= Decimal::ZERO {
        return raws.to_vec();
    }
    let n = raws.len();
    let mut base = vec![Decimal::ZERO; n];
    let mut rem = vec![Decimal::ZERO; n];
    let mut base_units = Decimal::ZERO;
    let sum_raw: Decimal = raws.iter().sum();
    for i in 0..n {
        let q = (raws[i] / step).floor();
        base[i] = q * step;
        rem[i] = raws[i] - base[i];
        base_units += q;
    }
    let target_units = (sum_raw / step).floor();
    let mut extra = target_units - base_units; // whole steps still to hand out
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| rem[b].cmp(&rem[a]));
    let mut result = base;
    let mut k = 0;
    while extra > Decimal::ZERO && k < n {
        result[order[k]] += step;
        extra -= Decimal::ONE;
        k += 1;
    }
    result
}

pub fn compute_dca_plan(
    categories: &[CategoryInput],
    budget: Decimal,
    rounding_step: Decimal,
) -> DcaPlan {
    let hundred = Decimal::from(100);
    let total: Decimal = categories.iter().map(|c| c.value_idr).sum();
    let raws = raw_allocations(categories, budget);
    let raw_totals: Vec<Decimal> = raws.iter().map(|r| r.rebalance + r.proportional).collect();
    let rounded = apply_rounding(&raw_totals, rounding_step);

    let mut lines = Vec::with_capacity(categories.len());
    let mut any_rebalance = false;
    let mut any_proportional = false;
    for (i, c) in categories.iter().enumerate() {
        let actual_pct = if total.is_zero() {
            Decimal::ZERO
        } else {
            c.value_idr / total * hundred
        };
        let phase = match (raws[i].rebalance > Decimal::ZERO, raws[i].proportional > Decimal::ZERO) {
            (true, true) => DcaPhase::Mixed,
            (true, false) => DcaPhase::Rebalance,
            (false, true) => DcaPhase::Proportional,
            (false, false) => DcaPhase::None,
        };
        if raws[i].rebalance > Decimal::ZERO {
            any_rebalance = true;
        }
        if raws[i].proportional > Decimal::ZERO {
            any_proportional = true;
        }
        lines.push(DcaCategoryLine {
            category_id: c.category_id,
            name: c.name.clone(),
            target_pct: c.target_pct,
            actual_pct,
            current_value_idr: c.value_idr,
            drift_pct: actual_pct - c.target_pct,
            allocate_idr: rounded[i],
            phase,
        });
    }

    let allocated: Decimal = lines.iter().map(|l| l.allocate_idr).sum();
    let mode = if allocated.is_zero() {
        DcaMode::Empty
    } else if any_rebalance && any_proportional {
        DcaMode::Mixed
    } else if any_rebalance {
        DcaMode::Rebalance
    } else {
        DcaMode::Proportional
    };

    let cash_leftover = budget - allocated;
    let note = match mode {
        DcaMode::Empty => {
            Some("Belum ada kategori target. Atur alokasi di halaman Rencana dulu.".to_string())
        }
        DcaMode::Proportional => Some(
            "Portfolio sudah dalam target — alokasi mengikuti proporsi target (mode proporsional)."
                .to_string(),
        ),
        _ if cash_leftover > Decimal::ZERO => Some(format!(
            "Sisa Rp {} tidak teralokasi (target di bawah 100% atau pembulatan).",
            cash_leftover
        )),
        _ => None,
    };

    DcaPlan {
        budget_idr: budget,
        total_value_idr: total,
        mode,
        lines,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn cat(id: i64, name: &str, target: Decimal, band: Option<Decimal>, value: Decimal) -> CategoryInput {
        CategoryInput {
            category_id: id,
            name: name.into(),
            target_pct: target,
            tolerance_band_pct: band,
            value_idr: value,
        }
    }

    // Helper: total allocated across all lines.
    fn allocated(plan: &DcaPlan) -> Decimal {
        plan.lines.iter().map(|l| l.allocate_idr).sum()
    }

    #[test]
    fn starves_over_allocated_category() {
        // V=200M; Crypto 30% (target 40), Saham 30% (target 35) -> both under;
        // Reksa 40% (target 25) -> over, must get 0. Budget 55M, no rounding (step 1).
        let cats = vec![
            cat(1, "Crypto", dec!(40), None, dec!(60000000)),
            cat(2, "Saham", dec!(35), None, dec!(60000000)),
            cat(3, "Reksa", dec!(25), None, dec!(80000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(55000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Rebalance);
        let reksa = plan.lines.iter().find(|l| l.category_id == 3).unwrap();
        assert_eq!(reksa.allocate_idr, dec!(0));
        assert_eq!(reksa.phase, DcaPhase::None);
        // budget fully consumed, tilted toward larger gap (Crypto gap 42M > Saham gap 29.25M)
        let crypto = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        let saham = plan.lines.iter().find(|l| l.category_id == 2).unwrap();
        assert!(crypto.allocate_idr > saham.allocate_idr);
        assert_eq!(allocated(&plan), dec!(55000000));
    }

    #[test]
    fn fills_gaps_then_proportional_when_budget_exceeds_gaps() {
        // Two categories slightly under; small total gap, big budget -> Phase 2 kicks in.
        // V=100M: A 45M (target 50), B 55M (target 50). Gap only on A.
        let cats = vec![
            cat(1, "A", dec!(50), None, dec!(45000000)),
            cat(2, "B", dec!(50), None, dec!(55000000)),
        ];
        // T = 150M. A target@T = 75M, gap 30M. B is over (55% > 50%) -> starved in phase 1.
        // budget 50M > gap 30M -> remainder 20M spread by target (50/50) = 10M each.
        let plan = compute_dca_plan(&cats, dec!(50000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Mixed);
        let a = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        let b = plan.lines.iter().find(|l| l.category_id == 2).unwrap();
        assert_eq!(a.allocate_idr, dec!(40000000)); // 30M gap + 10M proportional
        assert_eq!(a.phase, DcaPhase::Mixed);
        assert_eq!(b.allocate_idr, dec!(10000000)); // proportional only
        assert_eq!(b.phase, DcaPhase::Proportional);
        assert_eq!(allocated(&plan), dec!(50000000));
    }

    #[test]
    fn balanced_portfolio_is_pure_proportional() {
        // Already at target -> no gaps -> all budget proportional by target.
        let cats = vec![
            cat(1, "A", dec!(60), None, dec!(60000000)),
            cat(2, "B", dec!(40), None, dec!(40000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Proportional);
        let a = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        let b = plan.lines.iter().find(|l| l.category_id == 2).unwrap();
        assert_eq!(a.allocate_idr, dec!(6000000));
        assert_eq!(b.allocate_idr, dec!(4000000));
    }

    #[test]
    fn tolerance_band_is_a_deadzone() {
        // A is 48% vs target 50% (drift -2) within band 5 -> NOT rebalanced.
        // B is 52% vs target 50% -> over -> starved. All budget goes proportional.
        let cats = vec![
            cat(1, "A", dec!(50), Some(dec!(5)), dec!(48000000)),
            cat(2, "B", dec!(50), Some(dec!(5)), dec!(52000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Proportional);
        let a = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        assert_eq!(a.phase, DcaPhase::Proportional);
        assert_eq!(a.allocate_idr, dec!(5000000));
    }

    #[test]
    fn uncategorized_zero_target_gets_nothing() {
        let cats = vec![
            cat(1, "Crypto", dec!(100), None, dec!(50000000)),
            cat(-1, "Lainnya", dec!(0), None, dec!(50000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        let lainnya = plan.lines.iter().find(|l| l.category_id == -1).unwrap();
        assert_eq!(lainnya.allocate_idr, dec!(0));
    }

    #[test]
    fn target_under_100_leaves_cash() {
        // Single category, balanced, target 80 -> proportional gives 80% of budget, 20% stays cash.
        let cats = vec![cat(1, "A", dec!(80), None, dec!(80000000))];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        let a = &plan.lines[0];
        assert_eq!(a.allocate_idr, dec!(8000000));
        assert_eq!(allocated(&plan), dec!(8000000));
        assert!(plan.note.is_some()); // cash leftover reported
    }

    #[test]
    fn no_targets_is_empty_mode() {
        let cats = vec![cat(-1, "Lainnya", dec!(0), None, dec!(100000000))];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Empty);
        assert_eq!(allocated(&plan), dec!(0));
    }

    #[test]
    fn rounds_to_step_and_total_stays_within_budget() {
        // Three under-target categories with awkward gaps; step 10k.
        let cats = vec![
            cat(1, "A", dec!(40), None, dec!(10000000)),
            cat(2, "B", dec!(35), None, dec!(10000000)),
            cat(3, "C", dec!(25), None, dec!(10000000)),
        ];
        let budget = dec!(55000000);
        let step = dec!(10000);
        let plan = compute_dca_plan(&cats, budget, step);
        // every line is a whole multiple of the step
        for l in &plan.lines {
            assert_eq!(l.allocate_idr % step, dec!(0), "{} not a multiple of step", l.name);
        }
        // total never exceeds budget, and with targets summing to 100 it equals budget
        let total: Decimal = plan.lines.iter().map(|l| l.allocate_idr).sum();
        assert!(total <= budget);
        assert_eq!(total, budget);
    }
}
