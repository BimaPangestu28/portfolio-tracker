//! Indonesian rupiah amount in words, e.g. 12_000_000 -> "Dua belas juta rupiah".

const UNITS: [&str; 12] = [
    "nol", "satu", "dua", "tiga", "empat", "lima", "enam", "tujuh", "delapan",
    "sembilan", "sepuluh", "sebelas",
];

/// Lowercase words for a non-negative integer (no "rupiah").
fn spell(n: u64) -> String {
    match n {
        0..=11 => UNITS[n as usize].to_string(),
        12..=19 => format!("{} belas", UNITS[(n - 10) as usize]),
        20..=99 => {
            let tens = format!("{} puluh", UNITS[(n / 10) as usize]);
            if n % 10 == 0 { tens } else { format!("{tens} {}", UNITS[(n % 10) as usize]) }
        }
        100..=199 => {
            let rest = n % 100;
            if rest == 0 { "seratus".into() } else { format!("seratus {}", spell(rest)) }
        }
        200..=999 => {
            let hundreds = format!("{} ratus", UNITS[(n / 100) as usize]);
            let rest = n % 100;
            if rest == 0 { hundreds } else { format!("{hundreds} {}", spell(rest)) }
        }
        1000..=1999 => {
            let rest = n % 1000;
            if rest == 0 { "seribu".into() } else { format!("seribu {}", spell(rest)) }
        }
        2000..=999_999 => group(n, 1000, "ribu"),
        1_000_000..=999_999_999 => group(n, 1_000_000, "juta"),
        1_000_000_000..=999_999_999_999 => group(n, 1_000_000_000, "miliar"),
        _ => group(n, 1_000_000_000_000, "triliun"),
    }
}

/// "<spell(n/scale)> <unit> [spell(rest)]" for the thousands/millions/... groups.
fn group(n: u64, scale: u64, unit: &str) -> String {
    let head = format!("{} {unit}", spell(n / scale));
    let rest = n % scale;
    if rest == 0 { head } else { format!("{head} {}", spell(rest)) }
}

/// Capitalized amount in words with the "rupiah" suffix. Negatives clamp to 0.
pub fn terbilang(amount: i64) -> String {
    let words = spell(amount.max(0) as u64);
    let mut chars = words.chars();
    let capitalized = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => words,
    };
    format!("{capitalized} rupiah")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_the_template_and_boundaries() {
        assert_eq!(terbilang(0), "Nol rupiah");
        assert_eq!(terbilang(11), "Sebelas rupiah");
        assert_eq!(terbilang(12), "Dua belas rupiah");
        assert_eq!(terbilang(21), "Dua puluh satu rupiah");
        assert_eq!(terbilang(100), "Seratus rupiah");
        assert_eq!(terbilang(1000), "Seribu rupiah");
        assert_eq!(terbilang(2500), "Dua ribu lima ratus rupiah");
        assert_eq!(terbilang(12_000_000), "Dua belas juta rupiah");
        assert_eq!(
            terbilang(1_234_567),
            "Satu juta dua ratus tiga puluh empat ribu lima ratus enam puluh tujuh rupiah"
        );
        assert_eq!(terbilang(2_000_000_000), "Dua miliar rupiah");
        // "satu juta", never "se juta" — the se- prefix is only for belas-/ratus-/ribu-.
        assert_eq!(terbilang(1_000_000), "Satu juta rupiah");
        assert_eq!(terbilang(100_000), "Seratus ribu rupiah");
        assert_eq!(terbilang(1_100_000), "Satu juta seratus ribu rupiah");
        // Negatives clamp to zero.
        assert_eq!(terbilang(-1), "Nol rupiah");
    }
}
