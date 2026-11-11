//! Calendrier CMOS Q35. Le lanceur règle la RTC sur UTC.
use super::ports::Port;

fn register(index: u8) -> u8 {
    Port::RtcIndex.write(index);
    Port::RtcData.read()
}

fn sample() -> Option<[u8; 8]> {
    if register(0x0a) & 0x80 != 0 || register(0x0d) & 0x80 == 0 {
        return None;
    }
    let values = [0x0b, 0x32, 9, 8, 7, 4, 2, 0].map(register);
    (register(0x0a) & 0x80 == 0 && values[0] & 0x80 == 0).then_some(values)
}

pub(crate) fn seconds() -> Option<u64> {
    // Le BSP est l'unique lecteur CMOS. Masquer les IRQ empêche la préemption
    // entre la sélection du registre et la lecture de sa valeur.
    // Une mise à jour CMOS couvre plusieurs lectures de ports consécutives.
    // Le PIT (250 Hz) borne l'attente à deux ticks et les IRQ restent actives
    // entre les tentatives. Ces ticks ne servent jamais à dater un fichier.
    let start = crate::sched::ticks();
    let [mode, century, year, month, day, hour, minute, second] = loop {
        let flags = super::save_and_mask_interrupts();
        let pair = sample().and_then(|a| (sample() == Some(a)).then_some(a));
        super::restore_interrupts(flags);
        if let Some(stable) = pair {
            break stable;
        }
        if crate::sched::ticks().wrapping_sub(start) >= 2 {
            return None;
        }
        crate::sched::yield_now();
    };
    let decode = |v: u8| -> Option<u64> {
        if mode & 4 != 0 {
            return Some(v.into());
        }
        let (hi, lo) = (v >> 4, v & 15);
        (hi <= 9 && lo <= 9).then_some(u64::from(hi) * 10 + u64::from(lo))
    };
    let year = decode(century)? * 100 + decode(year)?;
    let month = decode(month)?;
    let day = decode(day)?;
    let hour = if mode & 2 != 0 {
        decode(hour)?
    } else {
        let h = decode(hour & 0x7f)?;
        if !(1..=12).contains(&h) {
            return None;
        }
        h % 12 + if hour & 0x80 != 0 { 12 } else { 0 }
    };
    let minute = decode(minute)?;
    let second = decode(second)?;
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || hour >= 24
        || minute >= 60
        || second >= 60
    {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let lengths = [
        31,
        28 + u64::from(leap),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=lengths[month as usize - 1]).contains(&day) {
        return None;
    }
    let leaps = |y: u64| y / 4 - y / 100 + y / 400;
    let days = (year - 1970) * 365 + leaps(year - 1) - leaps(1969)
        + lengths[..month as usize - 1].iter().sum::<u64>()
        + day
        - 1;
    Some(((days * 24 + hour) * 60 + minute) * 60 + second)
}
