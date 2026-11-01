//! Card number helpers: Luhn validation, brand detection and masking.

/// Returns true when the digits of `number` satisfy the Luhn checksum.
#[must_use]
pub fn luhn_valid(number: &str) -> bool {
    let digits: Vec<u32> = number.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 12 {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                *d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Detects the card brand from the IIN ranges used by the web vault.
#[must_use]
pub fn card_brand(number: &str) -> Option<&'static str> {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let two: u32 = digits.get(0..2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let four: u32 = digits.get(0..4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let one = digits.as_bytes()[0] - b'0';
    let brand = if one == 4 {
        "Visa"
    } else if (51..=55).contains(&two) || (2221..=2720).contains(&four) {
        "Mastercard"
    } else if matches!(two, 34 | 37) {
        "Amex"
    } else if four == 6011 || (644..=649).contains(&(four / 10)) || two == 65 {
        "Discover"
    } else if matches!(two, 36 | 38 | 39) || (300..=305).contains(&(four / 10)) {
        "Diners Club"
    } else if (3528..=3589).contains(&four) {
        "JCB"
    } else if (5000..=5099).contains(&four)
        || (5600..=5899).contains(&four)
        || (6200..=6299).contains(&four)
    {
        "Maestro"
    } else if (6200..=6299).contains(&four) || two == 62 {
        "UnionPay"
    } else if (6000..=6999).contains(&four) && (6521..=6522).contains(&four) {
        "RuPay"
    } else {
        "Other"
    };
    Some(brand)
}

/// Formats a card number in groups of four (Amex: 4-6-5), preserving only digits.
#[must_use]
pub fn mask_card_number(number: &str, revealed: bool) -> String {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    let brand = card_brand(&digits).unwrap_or("Other");
    if revealed {
        return group_card(&digits, brand);
    }
    let last4: String = digits.chars().skip(digits.len().saturating_sub(4)).collect();
    format!("•••• •••• •••• {last4}")
}

fn group_card(digits: &str, brand: &str) -> String {
    let groups: &[usize] = if brand == "Amex" { &[4, 6, 5] } else { &[4, 4, 4, 4, 3] };
    let mut out = String::new();
    let mut rest = digits;
    for (i, size) in groups.iter().enumerate() {
        if rest.is_empty() {
            break;
        }
        let take = (*size).min(rest.len());
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&rest[..take]);
        rest = &rest[take..];
    }
    if !rest.is_empty() {
        out.push(' ');
        out.push_str(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luhn_vectors() {
        assert!(luhn_valid("4242 4242 4242 4242"));
        assert!(luhn_valid("5555555555554444"));
        assert!(luhn_valid("378282246310005"));
        assert!(!luhn_valid("4242424242424241"));
        assert!(!luhn_valid("1234"));
    }

    #[test]
    fn brands() {
        assert_eq!(card_brand("4242424242424242"), Some("Visa"));
        assert_eq!(card_brand("5555555555554444"), Some("Mastercard"));
        assert_eq!(card_brand("378282246310005"), Some("Amex"));
        assert_eq!(card_brand("6011111111111117"), Some("Discover"));
        assert_eq!(card_brand("3530111333300000"), Some("JCB"));
        assert_eq!(card_brand(""), None);
    }

    #[test]
    fn masking_keeps_last_four() {
        assert_eq!(mask_card_number("4242424242424242", false), "•••• •••• •••• 4242");
        assert_eq!(mask_card_number("4242424242424242", true), "4242 4242 4242 4242");
        assert_eq!(mask_card_number("378282246310005", true), "3782 822463 10005");
    }
}
