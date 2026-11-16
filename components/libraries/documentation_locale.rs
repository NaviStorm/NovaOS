//! Choix pour la documentation. Une étiquette non française aboutit toujours à English.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    English,
    French,
}

impl Language {
    pub fn select<'a>(self, english: &'a str, french: &'a str) -> &'a str {
        match self {
            Self::English => english,
            Self::French => french,
        }
    }

    pub fn from_tag(tag: Option<&str>) -> Self {
        if tag.is_some_and(french_tag) {
            Self::French
        } else {
            Self::English
        }
    }
}

// RFC 5646 §2.1. Aucun tag grandfathered n'a « fr » comme langue principale.
// Cette décision dépend de la syntaxe, indépendamment des enregistrements IANA.
fn french_tag(tag: &str) -> bool {
    let mut parts = tag.split('-').peekable();
    if !parts.next().is_some_and(|s| s.eq_ignore_ascii_case("fr")) {
        return false;
    }
    let alpha = |s: &str| s.bytes().all(|b| b.is_ascii_alphabetic());
    let digit = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let alnum = |s: &str| s.bytes().all(|b| b.is_ascii_alphanumeric());
    for _ in 0..3 {
        if parts.next_if(|s| s.len() == 3 && alpha(s)).is_none() {
            break;
        }
    }
    parts.next_if(|s| s.len() == 4 && alpha(s));
    parts.next_if(|s| (s.len() == 2 && alpha(s)) || (s.len() == 3 && digit(s)));
    let mut variants: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
    while let Some(v) = parts.next_if(|s| {
        alnum(s)
            && ((5..=8).contains(&s.len()) || (s.len() == 4 && s.as_bytes()[0].is_ascii_digit()))
    }) {
        if variants.iter().any(|old| old.eq_ignore_ascii_case(v)) {
            return false;
        }
        variants.push(v);
    }
    let mut extensions = 0u64;
    while let Some(key) = parts.next() {
        if key.len() != 1 || !alnum(key) {
            return false;
        }
        if key.eq_ignore_ascii_case("x") {
            return parts.peek().is_some() && parts.all(|s| (1..=8).contains(&s.len()) && alnum(s));
        }
        let c = key.as_bytes()[0].to_ascii_lowercase();
        let index = if c.is_ascii_digit() {
            c - b'0'
        } else {
            10 + c - b'a'
        };
        let bit = 1u64 << index;
        if extensions & bit != 0 {
            return false;
        }
        extensions |= bit;
        let value = |s: &&str| (2..=8).contains(&s.len()) && alnum(s);
        if parts.next_if(value).is_none() {
            return false;
        }
        while parts.next_if(value).is_some() {}
    }
    true
}
