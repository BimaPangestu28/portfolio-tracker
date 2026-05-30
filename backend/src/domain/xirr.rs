use chrono::NaiveDate;

#[derive(Debug, Clone, Copy)]
pub struct CashFlow {
    pub date: NaiveDate,
    /// Negative = money out (invested), positive = money in (returned / current value).
    pub amount: f64,
}

fn npv(rate: f64, flows: &[CashFlow], t0: NaiveDate) -> f64 {
    flows
        .iter()
        .map(|f| {
            let years = (f.date - t0).num_days() as f64 / 365.0;
            f.amount / (1.0 + rate).powf(years)
        })
        .sum()
}

/// Returns annualized internal rate of return as a fraction (0.10 = 10%).
/// `None` if there is no sign change or it fails to converge.
pub fn xirr(flows: &[CashFlow]) -> Option<f64> {
    if flows.len() < 2 {
        return None;
    }
    let has_pos = flows.iter().any(|f| f.amount > 0.0);
    let has_neg = flows.iter().any(|f| f.amount < 0.0);
    if !(has_pos && has_neg) {
        return None;
    }

    let t0 = flows.iter().map(|f| f.date).min()?;

    let mut rate = 0.1;
    for _ in 0..100 {
        let f = npv(rate, flows, t0);
        let df = (npv(rate + 1e-6, flows, t0) - f) / 1e-6;
        if df.abs() < 1e-12 {
            break;
        }
        let next = rate - f / df;
        if !next.is_finite() {
            break;
        }
        if (next - rate).abs() < 1e-7 {
            return Some(next);
        }
        rate = next;
    }

    let (mut lo, mut hi) = (-0.9999_f64, 10.0_f64);
    let (mut flo, fhi) = (npv(lo, flows, t0), npv(hi, flows, t0));
    if flo * fhi > 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let fmid = npv(mid, flows, t0);
        if fmid.abs() < 1e-7 {
            return Some(mid);
        }
        if flo * fmid < 0.0 {
            hi = mid;
        } else {
            lo = mid;
            flo = fmid;
        }
    }
    Some((lo + hi) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn doubling_in_one_year_is_about_100pct() {
        let flows = vec![
            CashFlow { date: d(2025, 1, 1), amount: -100.0 },
            CashFlow { date: d(2026, 1, 1), amount: 200.0 },
        ];
        let r = xirr(&flows).unwrap();
        assert!((r - 1.0).abs() < 0.01, "got {r}");
    }

    #[test]
    fn flat_value_is_about_zero() {
        let flows = vec![
            CashFlow { date: d(2025, 1, 1), amount: -100.0 },
            CashFlow { date: d(2026, 1, 1), amount: 100.0 },
        ];
        let r = xirr(&flows).unwrap();
        assert!(r.abs() < 0.01, "got {r}");
    }

    #[test]
    fn no_sign_change_returns_none() {
        let flows = vec![
            CashFlow { date: d(2025, 1, 1), amount: -100.0 },
            CashFlow { date: d(2026, 1, 1), amount: -50.0 },
        ];
        assert!(xirr(&flows).is_none());
    }
}
