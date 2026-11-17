//! Données du parseur à destination de --debug, y compris la portion consommée.
use super::{Item, parse_item, relative::Relative};
use alloc::vec::Vec;
pub enum Piece {
    Date(Option<u32>, u8, u8),
    Time(u8, u8, u8, u32, Option<i32>),
    Weekday(i8, i32),
    Offset(i32),
    Relative([i64; 7]),
    Number,
}
pub struct Token<'a> {
    pub raw: &'a str,
    pub parts: Vec<Piece>,
}
fn date(d: super::date::Date) -> Piece {
    Piece::Date(d.year, d.month, d.day)
}
fn time(t: super::time::Time) -> Piece {
    Piece::Time(
        t.hour,
        t.minute,
        t.second,
        t.nanosecond,
        t.offset.map(|o| o.total_seconds()),
    )
}
pub fn inspect(input: &str) -> Vec<Token<'_>> {
    let lower = input.to_ascii_lowercase();
    let mut rest = lower.as_str();
    let mut tokens = Vec::new();
    while !rest.trim().is_empty() {
        let start = input.len() - rest.len();
        let Ok(item) = parse_item(&mut rest) else {
            break;
        };
        let end = input.len() - rest.len();
        if end == start {
            break;
        }
        let raw = input[start..end].trim();
        let mut parts = Vec::new();
        let part = match item {
            Item::Date(d) => date(d),
            Item::Time(t) => time(t),
            Item::DateTime(d) => {
                parts.push(date(d.date));
                time(d.time)
            }
            Item::Weekday(w) => Piece::Weekday(
                jiff::civil::Weekday::from(w.day).to_sunday_zero_offset(),
                w.offset,
            ),
            Item::Offset(o) => Piece::Offset(o.total_seconds()),
            Item::LocalZone | Item::TimeZone(_) => continue,
            Item::Relative(r) => {
                let mut a = [0i64; 7];
                match r {
                    Relative::Years(n) => a[0] = n.into(),
                    Relative::Months(n) => a[1] = n.into(),
                    Relative::Days(n) => a[2] = n.into(),
                    Relative::Hours(n) => a[3] = n.into(),
                    Relative::Minutes(n) => a[4] = n.into(),
                    Relative::Seconds(n, f) => {
                        a[5] = n;
                        a[6] = f.into()
                    }
                }
                Piece::Relative(a)
            }
            Item::Pure(_) => Piece::Number,
        };
        parts.push(part);
        tokens.push(Token { raw, parts });
    }
    tokens
}
