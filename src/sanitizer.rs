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

/// Duidelijke crisissignalen: niet plaatsen, maar naar 113 verwijzen.
/// Het AI-model mist deze vaak, dus dit is de eerste vangrail.
static CRISIS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)",
        r"zelfmoord|su[iï]cid|zelfdoding",
        r"|\beinde aan (?:te )?(?:mijn leven )?maken\b|\beinde aan mijn leven\b|\bmaak er een einde aan\b",
        r"|\bniet meer (?:willen )?leven\b|\bwil(?:de)? (?:niet meer leven|dood)\b",
        r"|\b(?:nooit|niet) meer wakker (?:te )?(?:willen )?worden\b",
        r"|\buit het leven (?:te )?stappen\b|\bstap eruit\b|\bvan het leven (?:te )?beroven\b",
        r"|\b(?:voor|onder) (?:de|een) trein\b",
        r"|\b(?:trein|brug|dak|flat)\b.{0,30}\bspring|\bspring\w*\b.{0,30}\b(?:trein|brug|dak|flat)\b",
        r"|afscheidsbrie(?:f|ven)",
        r"|\bpillen\b.{0,40}\b(?:klaar|genoeg|slikken|innemen|allemaal)\b",
        r"|\b(?:klaar|genoeg|slikken|innemen)\b.{0,40}\bpillen\b",
        r"|\bbeter af zonder mij\b|\bals ik er niet meer (?:ben|was|zou zijn)\b",
        r"|\bniemand (?:zou|zal) me (?:missen|merken)\b",
        r"|\b(?:mezelf|me) (?:snijden|snijd|iets aandoen|iets aan doen)\b|\bmet snijden\b|zelfbeschadig",
        r"|\bhet nut (?:van|om) (?:door ?(?:te )?gaan|verder ?(?:te )?gaan|leven|te leven)\b",
        r"|\bgeen zin meer (?:in|om te) leven\b",
    ))
    .unwrap()
});

/// Mogelijk zorgelijke woorden: wel opslaan, maar eerst langs het beheer
/// in plaats van direct openbaar. Losse woorden, dus 'doodeng' telt niet.
static REVIEW_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:dood|doodgaan|pillen|springen|snijden|afscheid|uitweg|opgeven|niet meer kunnen)\b")
        .unwrap()
});

pub fn is_explicit_crisis_trigger(input: &str) -> bool {
    CRISIS_REGEX.is_match(input)
}

pub fn needs_human_review(input: &str) -> bool {
    REVIEW_REGEX.is_match(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn herkent_crisiszinnen() {
        for zin in [
            "Ik wil niet meer leven.",
            "Ik denk er steeds vaker aan om er een einde aan te maken.",
            "Niemand zou het merken als ik er niet meer was.",
            "De pillen liggen al klaar. Vanavond is het genoeg geweest.",
            "Soms wil ik gaan slapen en nooit meer wakker worden.",
            "Ik zie het nut van doorgaan echt niet meer, voor niemand.",
            "Ik ben weer begonnen met snijden, het is het enige wat helpt.",
            "ik wil dood",
            "Iedereen is beter af zonder mij. Ik heb al afscheidsbrieven geschreven.",
            "Ik sta op het station en denk erover om voor de trein te springen.",
            "Ik denk aan zelfmoord.",
        ] {
            assert!(is_explicit_crisis_trigger(zin), "gemist: {zin}");
        }
    }

    #[test]
    fn gewone_berichten_worden_niet_geblokkeerd() {
        for zin in [
            "Deze maandag is echt dodelijk saai.",
            "Ik ga dood van het lachen om mijn collega.",
            "Mijn telefoon is dood en ik ben mijn oplader vergeten.",
            "Ik ben zo moe van alles en iedereen deze week.",
            "Nee zeggen vind ik doodeng. Dus ik zeg overal ja op, tot het ineens niet meer gaat.",
            "Buiten is het doodstil. In mijn hoofd is het spitsuur.",
            "Ik zou een week willen verdwijnen. Niet dramatisch, gewoon weg.",
            "Wat als ik echt instort en er dan niemand is?",
        ] {
            assert!(!is_explicit_crisis_trigger(zin), "onterecht geblokkeerd: {zin}");
        }
    }

    #[test]
    fn twijfelwoorden_gaan_naar_beheer() {
        assert!(needs_human_review("Ik ga dood van het lachen om mijn collega."));
        assert!(needs_human_review("Ik ben de pillen vergeten."));
        assert!(!needs_human_review("Nee zeggen vind ik doodeng."));
        assert!(!needs_human_review("Buiten is het doodstil."));
        assert!(!needs_human_review("Deze maandag is echt dodelijk saai."));
    }
}