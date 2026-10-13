use regex::Regex;
use std::sync::LazyLock;

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}").unwrap()
});

static PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?x)",
        r"(?:",
            r"(?:\+|00)(?:31|32)[\s./-]*(?:\(0\)[\s./-]*)?[1-9](?:[\s./-]*\d){7,9}",
            r"|",
            r"\b0[1-9](?:[\s./-]*\d){8}\b",
            r"|",
            r"\b0[1-9](?:[\s./-]*\d){7,8}\b",
        r")"
    )).unwrap()
});

static HANDLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@[\w]+").unwrap()
});

pub fn contains_obvious_pii(input: &str) -> bool {
    EMAIL_REGEX.is_match(input) || PHONE_REGEX.is_match(input) || HANDLE_REGEX.is_match(input)
}

pub fn is_explicit_crisis_trigger(input: &str) -> bool {
    let lower = input.to_lowercase();

    const STRICT_CRISIS_PHRASES: &[&str] = &[
        "zelfmoord",
        "suicide",
        "suïcide",
        "einde aan maken",
        "einde aan mijn leven",
        "niet meer wakker willen worden",
        "stap eruit vanavond",
        "uit het leven stappen",
        "ik wil niet meer leven",
        "ik maak er een einde aan",
    ];
    
    STRICT_CRISIS_PHRASES.iter().any(|&marker| lower.contains(marker))
}