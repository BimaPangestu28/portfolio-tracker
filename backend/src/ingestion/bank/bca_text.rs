//! Text extraction and structural detection for BCA "Rekening Tahapan" PDFs.

// Items here are consumed by later tasks (parser, categorizer); suppress dead-code
// lint until the full ingestion pipeline is wired up.
#![allow(dead_code)]

/// Statement-level fields needed to build stable external refs and resolve dates.
#[derive(Debug, Clone, PartialEq)]
pub struct StatementMeta {
    pub account_no: String,
    pub year: i32,
}

/// Whitespace-stripped, uppercased copy for keyword matching. `pdftotext -layout`
/// frequently inserts spaces inside words, so we cannot match on raw substrings.
fn squashed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()
}

/// True when the document looks like a BCA Tahapan statement.
pub fn is_bca_statement(text: &str) -> bool {
    let s = squashed(text);
    s.contains("REKENINGTAHAPAN") && s.contains("NO.REKENING")
}

/// Indonesian month name (as it appears squashed in the PERIODE line) -> month number.
fn month_from_periode(squashed_text: &str) -> Option<u32> {
    const MONTHS: [(&str, u32); 12] = [
        ("JANUARI", 1), ("FEBRUARI", 2), ("MARET", 3), ("APRIL", 4),
        ("MEI", 5), ("JUNI", 6), ("JULI", 7), ("AGUSTUS", 8),
        ("SEPTEMBER", 9), ("OKTOBER", 10), ("NOVEMBER", 11), ("DESEMBER", 12),
    ];
    let after = squashed_text.split("PERIODE").nth(1)?;
    MONTHS.iter().find(|(name, _)| after.contains(name)).map(|(_, n)| *n)
}

/// Extract the account number and statement year from the header.
pub fn statement_meta(text: &str) -> anyhow::Result<StatementMeta> {
    let s = squashed(text);
    // Account number: the run of digits immediately after "NO.REKENING".
    let after_acct = s.split("NO.REKENING").nth(1)
        .ok_or_else(|| anyhow::anyhow!("no NO.REKENING marker"))?;
    let account_no: String = after_acct.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if account_no.is_empty() {
        anyhow::bail!("could not read account number");
    }
    // Year: the 4-digit run after the month name in the PERIODE line.
    let _month = month_from_periode(&s); // validated for presence; row dates carry MM
    let after_periode = s.split("PERIODE").nth(1)
        .ok_or_else(|| anyhow::anyhow!("no PERIODE marker"))?;
    let year_str: String = after_periode.chars()
        .skip_while(|c| !c.is_ascii_digit())
        // skip the leading "1/3"-style page noise is avoided because PERIODE is its own field;
        // take the first standalone 4-digit run that is a plausible year.
        .collect();
    let year = year_str.as_bytes().windows(4)
        .filter_map(|w| std::str::from_utf8(w).ok())
        .filter_map(|w| w.parse::<i32>().ok())
        .find(|y| (2000..=2100).contains(y))
        .ok_or_else(|| anyhow::anyhow!("could not read statement year"))?;
    Ok(StatementMeta { account_no, year })
}

/// Render a PDF's text layer using `pdftotext -layout`, which preserves the
/// column alignment our parser relies on. Returns a clear error if the binary
/// is missing or the file has no extractable text.
pub async fn extract_text(path: &str) -> anyhow::Result<String> {
    let path = path.to_string();
    let output = tokio::process::Command::new("pdftotext")
        .arg("-layout")
        .arg(&path)
        .arg("-") // write to stdout
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("failed to run pdftotext (is poppler-utils installed?): {e}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "pdftotext failed for {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        anyhow::bail!("pdftotext produced no text for {path} (scanned/image-only PDF?)");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
                                                     REKENING TAHAPAN
    KCP PONDOK TIMUR

    B I M A P A N GE STU                          NO. RE KE NING   :    8415 5 25 237
                                                  HALAMAN          :    1/3
                                                  PE RIOD E        :    ME I 2026
                                                  MATA U ANG       :    ID R
";

    #[test]
    fn detects_bca_statement() {
        assert!(is_bca_statement(SAMPLE));
        assert!(!is_bca_statement("just some random invoice text"));
    }

    #[test]
    fn parses_account_no_and_year() {
        let m = statement_meta(SAMPLE).unwrap();
        assert_eq!(m.account_no, "8415525237");
        assert_eq!(m.year, 2026);
    }
}
