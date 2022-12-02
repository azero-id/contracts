pub fn is_name_allowed(domain: &str) -> bool {
    /* Alphanumeric only */
    domain.chars().all(|char| match char {
        'a'..='z' | '0'..='9' => true,
        _ => false,
    })
}

pub fn get_domain_price(domain: &str) -> u128 {
    match domain.len() {
        4 => 160 ^ 12,
        _ => 5 ^ 12,
    }
}
