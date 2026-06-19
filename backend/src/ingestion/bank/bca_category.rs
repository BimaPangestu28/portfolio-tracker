//! Maps a BCA mutation's KETERANGAN text to a cashflow category.

#[derive(Debug, Clone, PartialEq)]
pub struct BcaCategory {
    pub name: &'static str,
    pub kind: &'static str, // "income" | "expense"
    pub is_transfer: bool,
}

/// First matching rule wins. `kind` is the default used when the category is
/// first created; the cashflow row's own direction is what reporting keys on.
pub fn categorize(haystack: &str) -> BcaCategory {
    let h = haystack.to_uppercase();
    let has = |needle: &str| h.contains(needle);

    if has("TRSF E-BANKING") || has("FTFVA") || has("FTSCY") {
        BcaCategory { name: "Transfer", kind: "expense", is_transfer: true }
    } else if has("KARTU KREDIT") || has("BCA CARD") {
        BcaCategory { name: "Kartu Kredit", kind: "expense", is_transfer: false }
    } else if has("QRC") || has("QR ") || has("TRANSAKSI DEBIT") {
        BcaCategory { name: "Belanja/QRIS", kind: "expense", is_transfer: false }
    } else if has("BIAYA ADM") || has("ADMIN") {
        BcaCategory { name: "Biaya Bank", kind: "expense", is_transfer: false }
    } else if has("BUNGA") {
        BcaCategory { name: "Bunga", kind: "income", is_transfer: false }
    } else if has("PAJAK") {
        BcaCategory { name: "Pajak", kind: "expense", is_transfer: false }
    } else if has("SETORAN") {
        BcaCategory { name: "Setoran Tunai", kind: "income", is_transfer: false }
    } else {
        BcaCategory { name: "Lainnya", kind: "expense", is_transfer: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_is_flagged() {
        let c = categorize("TRSF E-BANKING DB 0105/FTFVA/WS95271 PT Moratelin");
        assert_eq!(c.name, "Transfer");
        assert!(c.is_transfer);
    }

    #[test]
    fn qris_merchant_is_belanja() {
        let c = categorize("TRANSAKSI DEBIT TGL: 01/05 QRC014 IDM INDOMA");
        assert_eq!(c.name, "Belanja/QRIS");
        assert!(!c.is_transfer);
    }

    #[test]
    fn credit_card_payment() {
        let c = categorize("KARTU KREDIT/PL 0100 BCA CARD BIMA PANGESTU");
        assert_eq!(c.name, "Kartu Kredit");
    }

    #[test]
    fn interest_and_fee_and_default() {
        assert_eq!(categorize("BUNGA").name, "Bunga");
        assert_eq!(categorize("BIAYA ADM").name, "Biaya Bank");
        assert_eq!(categorize("SOMETHING UNRECOGNIZED").name, "Lainnya");
    }

    // Fix B — SETORAN TUNAI must be categorized as income.
    #[test]
    fn setoran_tunai_is_income() {
        let c = categorize("SETORAN TUNAI 123456789");
        assert_eq!(c.name, "Setoran Tunai");
        assert_eq!(c.kind, "income");
        assert!(!c.is_transfer);
    }

    #[test]
    fn unknown_string_still_lainnya() {
        let c = categorize("RANDOM UNKNOWN TRANSACTION");
        assert_eq!(c.name, "Lainnya");
    }
}
