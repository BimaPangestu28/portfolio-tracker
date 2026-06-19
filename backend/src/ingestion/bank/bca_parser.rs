//! Deterministic parser for BCA Tahapan statements rendered with `pdftotext -layout`.

use crate::ingestion::bank::bca_text::StatementMeta;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    In,
    Out,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct BcaMutation {
    pub date: chrono::NaiveDate,
    pub jenis: String,
    pub deskripsi: String,
    pub amount: String,
    pub direction: Direction,
}

/// A line that opens a new mutation starts (after indentation) with `DD/MM`.
fn leading_date(line: &str) -> Option<(u32, u32)> {
    let t = line.trim_start();
    let bytes = t.as_bytes();
    if bytes.len() >= 5
        && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit()
        && bytes[2] == b'/'
        && bytes[3].is_ascii_digit() && bytes[4].is_ascii_digit()
    {
        let dd = t[0..2].parse().ok()?;
        let mm = t[3..5].parse().ok()?;
        return Some((dd, mm));
    }
    None
}

/// Pull the first money-like token (`1,234.56`) and whether it is a debit
/// (trailing " DB"). Returns the normalized amount (no separators) + direction.
fn money_and_direction(rest: &str) -> Option<(String, Direction)> {
    // Find a token matching <digits with commas>.<2 digits>.
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    for (i, tok) in tokens.iter().enumerate() {
        let cleaned: String = tok.chars().filter(|c| *c != ',').collect();
        if cleaned.contains('.')
            && cleaned.chars().all(|c| c.is_ascii_digit() || c == '.')
            && cleaned.split('.').nth(1).map(|f| f.len() == 2).unwrap_or(false)
        {
            // Debit if this token (or the next) is "DB".
            let is_db = tok.ends_with("DB")
                || tokens.get(i + 1).map(|n| *n == "DB").unwrap_or(false);
            let direction = if is_db { Direction::Out } else { Direction::In };
            // The first money token is MUTASI; a later one would be SALDO — we
            // only take the first, which is always the transaction amount.
            return Some((cleaned, direction));
        }
    }
    None
}

/// The KETERANGAN "type" sits between the date and the first detail/amount.
/// In `-layout` output it is the run of words after the date column and before
/// the long whitespace gap that precedes the detail/MUTASI columns.
fn split_jenis_and_first_detail(after_date: &str) -> (String, String) {
    // Columns are separated by 2+ spaces. First chunk after the date is jenis;
    // the remainder (detail + amount) we keep raw for description harvesting.
    let trimmed = after_date.trim_start();
    // jenis = leading words up to a run of 2+ spaces.
    if let Some(gap) = trimmed.find("  ") {
        let jenis = trimmed[..gap].trim().to_string();
        let detail = trimmed[gap..].trim().to_string();
        (jenis, detail)
    } else {
        (trimmed.trim().to_string(), String::new())
    }
}

/// Strip the money/SALDO tail off a detail line so descriptions stay clean.
fn detail_without_money(detail: &str) -> String {
    detail
        .split_whitespace()
        .take_while(|t| {
            let cleaned: String = t.chars().filter(|c| *c != ',').collect();
            !(cleaned.contains('.')
                && cleaned.chars().all(|c| c.is_ascii_digit() || c == '.'))
                && *t != "DB"
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(dead_code)]
pub fn parse_mutations(text: &str, meta: &StatementMeta) -> Vec<BcaMutation> {
    let mut out: Vec<BcaMutation> = Vec::new();
    for line in text.lines() {
        if let Some((dd, mm)) = leading_date(line) {
            let t = line.trim_start();
            let after_date = &t[5..]; // skip "DD/MM"
            let (jenis, first_detail) = split_jenis_and_first_detail(after_date);
            if jenis.eq_ignore_ascii_case("SALDO AWAL") {
                continue;
            }
            let Some(date) = chrono::NaiveDate::from_ymd_opt(meta.year, mm, dd) else {
                continue;
            };
            match money_and_direction(after_date) {
                Some((amount, direction)) => {
                    out.push(BcaMutation {
                        date,
                        jenis,
                        deskripsi: detail_without_money(&first_detail),
                        amount,
                        direction,
                    });
                }
                None => {
                    // A dated row with no money token is malformed; record it
                    // with a zero amount so it surfaces in review rather than
                    // vanishing.
                    out.push(BcaMutation {
                        date,
                        jenis,
                        deskripsi: detail_without_money(&first_detail),
                        amount: "0.00".to_string(),
                        direction: Direction::Out,
                    });
                }
            }
        } else if let Some(last) = out.last_mut() {
            // Continuation line: append non-money detail to the current mutation.
            let extra = detail_without_money(line.trim());
            if !extra.is_empty() {
                if !last.deskripsi.is_empty() {
                    last.deskripsi.push(' ');
                }
                last.deskripsi.push_str(&extra);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::bank::bca_text::StatementMeta;

    // Real `pdftotext -layout` shape: date in the left column opens a row;
    // continuation lines have no leading date; MUTASI carries a trailing " DB"
    // for debits and nothing for credits.
    const ROWS: &str = "\
       01/05         SALDO AWAL                                                          4,153,064.29
       01/05         TRSF E-BANKING DB    0105/FTFVA/WS95271                242,000.00 DB     3,911,064.29
                                          38165/PT Moratelin
       01/05         TRANSAKSI DEBIT      TGL: 01/05                        137,000.00 DB
                                          QRC014
                                          00000.00IDM INDOMA
       12/05         TRSF E-BANKING CR    1205/FTSCY/WS95051             49,995,500.00        40,831,664.29
                                          SINAR DIGITAL TERD
";

    fn meta() -> StatementMeta { StatementMeta { account_no: "8415525237".into(), year: 2026 } }

    #[test]
    fn skips_saldo_awal() {
        let m = parse_mutations(ROWS, &meta());
        assert!(m.iter().all(|x| !x.jenis.contains("SALDO AWAL")));
    }

    #[test]
    fn parses_three_mutations_with_direction_and_amount() {
        let m = parse_mutations(ROWS, &meta());
        assert_eq!(m.len(), 3);

        assert_eq!(m[0].jenis, "TRSF E-BANKING DB");
        assert_eq!(m[0].amount, "242000.00");
        assert!(matches!(m[0].direction, Direction::Out));
        assert_eq!(m[0].date, chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert!(m[0].deskripsi.contains("PT Moratelin"));

        assert_eq!(m[1].jenis, "TRANSAKSI DEBIT");
        assert_eq!(m[1].amount, "137000.00");
        assert!(matches!(m[1].direction, Direction::Out));
        assert!(m[1].deskripsi.contains("QRC014"));

        assert_eq!(m[2].jenis, "TRSF E-BANKING CR");
        assert_eq!(m[2].amount, "49995500.00");
        assert!(matches!(m[2].direction, Direction::In));
    }
}
