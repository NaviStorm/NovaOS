//! Dates UTC du service P9 et pagination locale de l'ELF nu.
use super::*;
pub struct Date {
    year: i64,
    month: usize,
    day: i64,
    weekday: usize,
    h: i64,
    m: i64,
    s: i64,
}
impl Date {
    pub fn utc(seconds: i64) -> Self {
        let days = seconds.div_euclid(86400);
        let day = days + 719468;
        let era = day.div_euclid(146097);
        let n = day.rem_euclid(146097);
        let y = (n - n / 1460 + n / 36524 - n / 146096) / 365;
        let ordinal = n - (365 * y + y / 4 - y / 100);
        let m = (5 * ordinal + 2) / 153;
        let month = if m < 10 { m + 3 } else { m - 9 };
        let clock = seconds.rem_euclid(86400);
        Self {
            year: y + era * 400 + i64::from(month <= 2),
            month: month as usize,
            day: ordinal - (153 * m + 2) / 5 + 1,
            weekday: (days + 4).rem_euclid(7) as usize,
            h: clock / 3600,
            m: clock / 60 % 60,
            s: clock % 60,
        }
    }
    fn month(&self) -> &'static str {
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ][self.month - 1]
    }
    pub fn stamp(&self, ns: u32, context: bool) -> Vec<u8> {
        if context {
            let week = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][self.weekday];
            format!(
                "{week} {} {:2} {:02}:{:02}:{:02} {}",
                self.month(),
                self.day,
                self.h,
                self.m,
                self.s,
                self.year
            )
            .into_bytes()
        } else {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{ns:09} +0000",
                self.year, self.month, self.day, self.h, self.m, self.s
            )
            .into_bytes()
        }
    }
    pub fn paginate(&self, title: &[u8], data: &[u8]) -> Vec<u8> {
        let lines: Vec<_> = data.split_inclusive(|c| *c == b'\n').collect();
        let mut result = Vec::new();
        for (n, page) in lines.chunks(56).enumerate() {
            result.extend(
                format!(
                    "\n\n{} {:2} {:02}:{:02} {} ",
                    self.month(),
                    self.day,
                    self.h,
                    self.m,
                    self.year
                )
                .bytes(),
            );
            result.extend(title);
            result.extend(format!(" Page {}\n\n\n", n + 1).bytes());
            for s in page {
                result.extend(*s);
                if !s.ends_with(b"\n") {
                    result.push(b'\n')
                }
            }
            result.resize(result.len() + 61 - page.len(), b'\n');
        }
        result
    }
}
