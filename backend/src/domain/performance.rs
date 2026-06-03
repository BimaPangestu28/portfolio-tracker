//! Time-weighted return (TWR) curve + risk metrics.
//!
//! Pure functions over a NAV series and an external-cashflow series. TWR daily-
//! links interval returns that exclude external flows:
//!   r = (NAV_end - flow_in_interval) / NAV_start - 1
//! so deposits/withdrawals don't show up as gains/losses.

use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct PerfPoint {
    pub date: NaiveDate,
    pub cum_return: f64,
    pub nav: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PerfMetrics {
    pub total_return: f64,
    pub annualized: f64,
    pub max_drawdown: f64,
    pub volatility: f64,
}

const EMPTY_METRICS: PerfMetrics = PerfMetrics {
    total_return: 0.0,
    annualized: 0.0,
    max_drawdown: 0.0,
    volatility: 0.0,
};

/// Largest peak-to-trough decline of a wealth index (<= 0).
pub fn max_drawdown(wealth: &[f64]) -> f64 {
    let mut peak = f64::MIN;
    let mut worst = 0.0_f64;
    for &w in wealth {
        if w > peak {
            peak = w;
        }
        if peak > 0.0 {
            let dd = w / peak - 1.0;
            if dd < worst {
                worst = dd;
            }
        }
    }
    worst
}

/// Sample standard deviation. Returns 0 for fewer than 2 points.
fn stdev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0);
    var.sqrt()
}

/// Sum of flows falling in the half-open interval `(prev, cur]`.
fn flow_in(flows: &[(NaiveDate, f64)], prev: NaiveDate, cur: NaiveDate) -> f64 {
    flows
        .iter()
        .filter(|(date, _)| *date > prev && *date <= cur)
        .map(|(_, amt)| *amt)
        .sum()
}

/// Build the cumulative-return series and metrics. `navs` must be sorted by date.
/// Returns empty points + zero metrics when there are < 2 usable snapshots.
pub fn compute(
    navs: &[(NaiveDate, f64)],
    flows: &[(NaiveDate, f64)],
) -> (Vec<PerfPoint>, PerfMetrics) {
    // Start at the first snapshot with a positive NAV.
    let start = match navs.iter().position(|(_, v)| *v > 0.0) {
        Some(i) => i,
        None => return (Vec::new(), EMPTY_METRICS),
    };
    let series = &navs[start..];
    if series.len() < 2 {
        return (Vec::new(), EMPTY_METRICS);
    }

    let mut wealth = 1.0_f64;
    let mut wealth_series = vec![1.0_f64];
    let mut returns: Vec<f64> = Vec::new();
    let mut points = vec![PerfPoint {
        date: series[0].0,
        cum_return: 0.0,
        nav: series[0].1,
    }];

    for w in series.windows(2) {
        let (prev_date, v_prev) = w[0];
        let (cur_date, v_cur) = w[1];
        let f = flow_in(flows, prev_date, cur_date);
        let r = if v_prev > 0.0 {
            (v_cur - f) / v_prev - 1.0
        } else {
            0.0
        };
        returns.push(r);
        wealth *= 1.0 + r;
        wealth_series.push(wealth);
        points.push(PerfPoint {
            date: cur_date,
            cum_return: wealth - 1.0,
            nav: v_cur,
        });
    }

    let total_return = wealth - 1.0;
    let span_days = (series.last().unwrap().0 - series[0].0).num_days().max(1) as f64;
    let annualized = if wealth > 0.0 {
        wealth.powf(365.0 / span_days) - 1.0
    } else {
        -1.0
    };
    let avg_interval = span_days / returns.len() as f64;
    let periods_per_year = if avg_interval > 0.0 {
        365.0 / avg_interval
    } else {
        0.0
    };
    let volatility = stdev(&returns) * periods_per_year.sqrt();

    (
        points,
        PerfMetrics {
            total_return,
            annualized,
            max_drawdown: max_drawdown(&wealth_series),
            volatility,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn deposit_only_yields_zero_return() {
        // NAV doubled but only because of a 100 deposit -> TWR must be 0.
        let navs = vec![(d("2026-01-01"), 100.0), (d("2026-01-02"), 200.0)];
        let flows = vec![(d("2026-01-02"), 100.0)];
        let (points, m) = compute(&navs, &flows);
        assert!((points.last().unwrap().cum_return).abs() < 1e-9);
        assert!(m.total_return.abs() < 1e-9);
    }

    #[test]
    fn pure_gain_yields_that_return() {
        let navs = vec![(d("2026-01-01"), 100.0), (d("2026-01-02"), 110.0)];
        let (_p, m) = compute(&navs, &[]);
        assert!((m.total_return - 0.10).abs() < 1e-9);
    }

    #[test]
    fn withdrawal_is_not_a_loss() {
        // NAV fell 100 -> 90 only because 10 was withdrawn -> 0 return.
        let navs = vec![(d("2026-01-01"), 100.0), (d("2026-01-02"), 90.0)];
        let flows = vec![(d("2026-01-02"), -10.0)];
        let (_p, m) = compute(&navs, &flows);
        assert!(m.total_return.abs() < 1e-9);
    }

    #[test]
    fn max_drawdown_of_known_wealth_series() {
        // wealth peaks at 1.1 then drops to 0.88 -> dd = 0.88/1.1 - 1 = -0.2
        let wealth = vec![1.0, 1.1, 0.88, 0.924];
        assert!((max_drawdown(&wealth) - (-0.2)).abs() < 1e-9);
    }

    #[test]
    fn fewer_than_two_navs_is_empty() {
        let (points, m) = compute(&[(d("2026-01-01"), 100.0)], &[]);
        assert!(points.is_empty());
        assert_eq!(m.total_return, 0.0);
    }
}
