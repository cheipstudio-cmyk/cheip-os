//! Calcoli del calendario (UI in inglese), settimana che parte da lunedì.

use chrono::{Datelike, NaiveDate};

pub const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];
pub const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let first_next = NaiveDate::from_ymd_opt(ny, nm, 1).expect("data valida");
    first_next.pred_opt().expect("data valida").day()
}

/// Colonna (0 = lunedì) del primo giorno del mese
pub fn first_column(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month, 1).expect("data valida").weekday().num_days_from_monday()
}

pub fn iso(year: i32, month: u32, day: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// "Wednesday, November 25"
pub fn long_label(d: NaiveDate) -> String {
    format!("{}, {} {}", WEEKDAYS[d.weekday().num_days_from_monday() as usize], MONTHS[d.month0() as usize], d.day())
}

/// "Nov 25"
pub fn short_label(d: NaiveDate) -> String {
    format!("{} {}", &MONTHS[d.month0() as usize][..3], d.day())
}

pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let idx = year * 12 + month as i32 - 1 + delta;
    (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32)
}

/// "18.30", "1830", "18" diventano "18:30" / "18:00"
pub fn normalize_time(s: &str) -> String {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    let (h, m) = match digits.len() {
        0 => return "--:--".into(),
        1 | 2 => (digits.parse::<u32>().unwrap_or(0), 0),
        3 => (digits[..1].parse().unwrap_or(0), digits[1..].parse().unwrap_or(0)),
        _ => (digits[..2].parse().unwrap_or(0), digits[2..4].parse().unwrap_or(0)),
    };
    format!("{:02}:{:02}", h.min(23), m.min(59))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn november_2026() {
        assert_eq!(days_in_month(2026, 11), 30);
        assert_eq!(first_column(2026, 11), 6, "1 novembre 2026 è domenica");
        assert_eq!(days_in_month(2028, 2), 29);
    }

    #[test]
    fn labels_and_shift() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(long_label(d), "Wednesday, November 25");
        assert_eq!(short_label(d), "Nov 25");
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
    }

    #[test]
    fn times() {
        assert_eq!(normalize_time("18.30"), "18:30");
        assert_eq!(normalize_time("9"), "09:00");
        assert_eq!(normalize_time("930"), "09:30");
        assert_eq!(normalize_time(""), "--:--");
    }
}
