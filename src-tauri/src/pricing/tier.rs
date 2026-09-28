//! 计费档判定（DeepSeek 峰谷定价）：按每行 `started_at`（请求开始时刻，
//! Unix 毫秒 / UTC）判 Peak / Off-Peak。纯整数实现，不引入 chrono。
//!
//! 规则（内置常量，依据国办发明电〔2025〕7号，已与仓库所有者确认）：
//! - 北京时间（UTC+8 固定偏移，中国无夏令时）周一至周五 09:00–12:00、
//!   14:00–18:00，且非法定节假日 → Peak；
//! - 其余（夜间、午休、周末、节假日、调休补班周末日）→ Off-Peak。
//!
//! 节假日表仅覆盖 2026-09-01 ~ 2027-01-31（所有者确认的范围）；之后需手动
//! 维护本表，表外日期退化为按周几判定。补班日均在周末，判档天然按
//! Off-Peak 处理，无需记录。

const MS_PER_DAY: i64 = 86_400_000;

/// 自 1970-01-01（含）的天数。Howard Hinnant 的 `days_from_civil` 算法
/// （纯整数，proleptic Gregorian 历）。`const fn` 使节假日常量表可在编译期求值。
const fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;                          // [0, 399]
    let mp = (m as i64 + 9) % 12;                     // Mar=0 .. Feb=11
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;      // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;  // [0, 146096]
    era * 146097 + doe - 719468
}

/// 落在周一至周五的法定节假日（自 1970-01-01 的天数，逐日闭表列举）。
/// 范围仅 2026-09-01 ~ 2027-01-31；补班日均在周末，无需记录。
const HOLIDAY_DAYS: &[i64] = &[
    // 中秋 2026-09-25（周五）~ 09-27（周日），不调休
    days_from_civil(2026, 9, 25),
    days_from_civil(2026, 9, 26),
    days_from_civil(2026, 9, 27),
    // 国庆 2026-10-01（周四）~ 10-07（周三）；补班 09-20、10-10 均为周末
    days_from_civil(2026, 10, 1),
    days_from_civil(2026, 10, 2),
    days_from_civil(2026, 10, 3),
    days_from_civil(2026, 10, 4),
    days_from_civil(2026, 10, 5),
    days_from_civil(2026, 10, 6),
    days_from_civil(2026, 10, 7),
    // 元旦 2027-01-01（周五）~ 01-03（周日）
    days_from_civil(2027, 1, 1),
    days_from_civil(2027, 1, 2),
    days_from_civil(2027, 1, 3),
];

/// 该请求时刻是否处于高峰档（Peak）。
pub fn is_peak(started_at_ms: i64) -> bool {
    // 北京时间 = UTC + 8h（固定偏移）。div/rem_euclid 保证负时间戳也正确拆分。
    let beijing = started_at_ms + 8 * 3_600_000;
    let days = beijing.div_euclid(MS_PER_DAY);
    let day_ms = beijing.rem_euclid(MS_PER_DAY);
    // 1970-01-01 是周四：(days + 3) mod 7 → 0=周一 .. 6=周日
    let weekday = (days + 3).rem_euclid(7);
    if weekday > 4 {
        return false; // 周六 / 周日（含周末补班日：全天空闲档）
    }
    let in_morning = 9 * 3_600_000 <= day_ms && day_ms < 12 * 3_600_000;
    let in_afternoon = 14 * 3_600_000 <= day_ms && day_ms < 18 * 3_600_000;
    if !(in_morning || in_afternoon) {
        return false; // 午休 / 夜间
    }
    !HOLIDAY_DAYS.contains(&days)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 北京时间 y-m-d h:min:s.ms 对应的 Unix 毫秒（UTC）。
    fn at(y: i64, m: u32, d: u32, h: u32, min: u32, s: u32, ms: u32) -> i64 {
        days_from_civil(y, m, d) * MS_PER_DAY
            + (h as i64) * 3_600_000
            + (min as i64) * 60_000
            + (s as i64) * 1000
            + (ms as i64)
            - 8 * 3_600_000
    }

    #[test]
    fn days_from_civil_matches_known_values() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        // Howard Hinnant 论文中的示例值
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
    }

    /// weekday 约定 0=周一（1970-01-01 是周四=3）。用真实日历核对。
    #[test]
    fn weekday_zero_is_monday() {
        let wd = |y, m, d| (days_from_civil(y, m, d) + 3).rem_euclid(7);
        assert_eq!(wd(1970, 1, 1), 3, "1970-01-01 是周四");
        assert_eq!(wd(2026, 9, 25), 4, "2026-09-25 是周五（中秋）");
        assert_eq!(wd(2026, 9, 26), 5, "2026-09-26 是周六");
        assert_eq!(wd(2026, 9, 27), 6, "2026-09-27 是周日");
        assert_eq!(wd(2026, 10, 1), 3, "2026-10-01 是周四");
        assert_eq!(wd(2026, 10, 5), 0, "2026-10-05 是周一");
        assert_eq!(wd(2027, 1, 1), 4, "2027-01-01 是周五");
        assert_eq!(wd(2027, 2, 1), 0, "2027-02-01 是周一");
    }

    /// 工作日峰段边界：09:00 含、08:59 不含；12:00 不含（午休）；
    /// 13:59 不含、14:00 含；18:00 不含。2026-09-28 为周一（非节假日）。
    #[test]
    fn peak_window_boundaries_on_workday() {
        let mon = |h, min, s, ms| at(2026, 9, 28, h, min, s, ms);
        assert!(is_peak(mon(9, 0, 0, 0)), "09:00:00.000 应含");
        assert!(!is_peak(mon(8, 59, 59, 999)), "08:59:59.999 不应含");
        assert!(is_peak(mon(11, 59, 59, 999)), "11:59:59.999 应含");
        assert!(!is_peak(mon(12, 0, 0, 0)), "12:00 午休不应含");
        assert!(!is_peak(mon(13, 59, 59, 999)), "13:59 不应含");
        assert!(is_peak(mon(14, 0, 0, 0)), "14:00 应含");
        assert!(is_peak(mon(17, 59, 59, 999)), "17:59:59.999 应含");
        assert!(!is_peak(mon(18, 0, 0, 0)), "18:00 不应含");
        // 工作日非峰段：清晨与夜间
        assert!(!is_peak(mon(0, 30, 0, 0)));
        assert!(!is_peak(mon(7, 30, 0, 0)));
        assert!(!is_peak(mon(23, 30, 0, 0)));
    }

    /// 周末全天不为峰（含峰段时刻抽查）。
    #[test]
    fn weekends_are_never_peak() {
        for (y, m, d) in [(2026, 9, 26), (2026, 9, 27), (2027, 2, 6)] {
            for h in [0u32, 9, 10, 12, 15, 17, 23] {
                assert!(
                    !is_peak(at(y, m, d, h, 30, 0, 0)),
                    "{y}-{m:02}-{d:02} {h}:30 周末不应为峰"
                );
            }
        }
    }

    /// 表内节假日即便落在工作日峰段也不为峰。
    #[test]
    fn holidays_in_table_are_never_peak() {
        assert!(!is_peak(at(2026, 10, 5, 10, 0, 0, 0)), "国庆 10-05 周一 10:00 不应含");
        assert!(!is_peak(at(2026, 9, 25, 10, 0, 0, 0)), "中秋 09-25 周五 10:00 不应含");
        assert!(!is_peak(at(2027, 1, 1, 10, 0, 0, 0)), "元旦 01-01 周五 10:00 不应含");
        assert!(!is_peak(at(2026, 10, 1, 15, 0, 0, 0)), "国庆 10-01 周四 15:00 不应含");
    }

    /// UTC → 北京换算：UTC 01:30 = 北京 09:30 → Peak；
    /// UTC 23:30 = 北京次日 07:30（跨日）→ 非 Peak。
    #[test]
    fn utc_to_beijing_conversion() {
        // UTC 2026-09-28 01:30（周一）
        let utc_0130 = days_from_civil(2026, 9, 28) * MS_PER_DAY + (1 * 3_600 + 30 * 60) * 1000;
        assert!(is_peak(utc_0130), "UTC 01:30 = 北京 09:30 应为峰");
        // UTC 2026-09-28 23:30 = 北京 2026-09-29（周二）07:30
        let utc_2330 = days_from_civil(2026, 9, 28) * MS_PER_DAY + (23 * 3_600 + 30 * 60) * 1000;
        assert!(!is_peak(utc_2330), "UTC 23:30 = 北京次日 07:30 不应为峰");
    }

    /// 节假日表范围外（2027-02 起）按周几正常判定。
    #[test]
    fn dates_outside_table_fall_back_to_weekday_rule() {
        assert!(is_peak(at(2027, 2, 1, 10, 0, 0, 0)), "2027-02-01 周一 10:00 应为峰");
        assert!(is_peak(at(2027, 2, 15, 15, 0, 0, 0)), "2027-02-15 周一 15:00 应为峰");
        assert!(!is_peak(at(2027, 2, 6, 10, 0, 0, 0)), "2027-02-06 周六不应为峰");
        // 远古日期同样按周几判定
        assert!(is_peak(at(1970, 1, 1, 10, 0, 0, 0)), "1970-01-01 周四 10:00 应为峰");
        assert!(!is_peak(at(1970, 1, 3, 10, 0, 0, 0)), "1970-01-03 周六不应为峰");
    }
}
