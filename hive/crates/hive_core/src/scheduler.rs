use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CronSchedule — basic cron expression parser
// ---------------------------------------------------------------------------

/// Parsed representation of a cron expression with five fields:
/// `minute hour day-of-month month day-of-week`.
///
/// Supports:
/// - Literal values: `5`
/// - Wildcards: `*`
/// - Step values: `*/5`
/// - Ranges: `1-5`
/// - Lists (comma-separated): `1,3,5`
/// - Combinations: `1-5,10,*/15`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronSchedule {
    pub minutes: Vec<u32>,
    pub hours: Vec<u32>,
    pub days_of_month: Vec<u32>,
    pub months: Vec<u32>,
    pub days_of_week: Vec<u32>,
}

impl CronSchedule {
    /// Parse a standard five-field cron expression.
    ///
    /// ```text
    /// ┌───────── minute (0-59)
    /// │ ┌─────── hour (0-23)
    /// │ │ ┌───── day of month (1-31)
    /// │ │ │ ┌─── month (1-12)
    /// │ │ │ │ ┌─ day of week (0-6, 0 = Sunday)
    /// │ │ │ │ │
    /// * * * * *
    /// ```
    pub fn parse(expr: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            anyhow::bail!(
                "Invalid cron expression: expected 5 fields, got {}",
                parts.len()
            );
        }

        let minutes = parse_field(parts[0], 0, 59)?;
        let hours = parse_field(parts[1], 0, 23)?;
        let days_of_month = parse_field(parts[2], 1, 31)?;
        let months = parse_field(parts[3], 1, 12)?;
        let days_of_week = parse_field(parts[4], 0, 6)?;

        Ok(Self {
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
        })
    }

    /// Returns `true` if the given datetime matches this schedule.
    pub fn matches(&self, dt: &DateTime<Utc>) -> bool {
        let minute = dt.minute();
        let hour = dt.hour();
        let day = dt.day();
        let month = dt.month();
        let weekday = dt.weekday().num_days_from_sunday(); // 0 = Sunday

        self.minutes.contains(&minute)
            && self.hours.contains(&hour)
            && self.days_of_month.contains(&day)
            && self.months.contains(&month)
            && self.days_of_week.contains(&weekday)
    }
}

/// Parse a single cron field into a sorted, deduplicated list of values.
fn parse_field(field: &str, min: u32, max: u32) -> anyhow::Result<Vec<u32>> {
    let mut values = Vec::new();

    for part in field.split(',') {
        let part = part.trim();
        if part.contains('/') {
            // Step: "*/5" or "1-10/2"
            let (range_part, step_str) = part
                .split_once('/')
                .ok_or_else(|| anyhow::anyhow!("Invalid step expression: {}", part))?;
            let step: u32 = step_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid step value: {}", step_str))?;
            if step == 0 {
                anyhow::bail!("Step value must be > 0 in: {}", part);
            }

            let (start, end) = if range_part == "*" {
                (min, max)
            } else if range_part.contains('-') {
                parse_range(range_part, min, max)?
            } else {
                let v: u32 = range_part
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid value: {}", range_part))?;
                (v, max)
            };

            let mut v = start;
            while v <= end {
                values.push(v);
                v += step;
            }
        } else if part.contains('-') {
            // Range: "1-5"
            let (start, end) = parse_range(part, min, max)?;
            for v in start..=end {
                values.push(v);
            }
        } else if part == "*" {
            // Wildcard
            for v in min..=max {
                values.push(v);
            }
        } else {
            // Literal
            let v: u32 = part
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid cron value: {}", part))?;
            if v < min || v > max {
                anyhow::bail!("Value {} out of range [{}, {}]", v, min, max);
            }
            values.push(v);
        }
    }

    values.sort_unstable();
    values.dedup();

    if values.is_empty() {
        anyhow::bail!("Cron field produced no values: {}", field);
    }

    Ok(values)
}

fn parse_range(s: &str, min: u32, max: u32) -> anyhow::Result<(u32, u32)> {
    let (start_str, end_str) = s
        .split_once('-')
        .ok_or_else(|| anyhow::anyhow!("Invalid range: {}", s))?;
    let start: u32 = start_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid range start: {}", start_str))?;
    let end: u32 = end_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid range end: {}", end_str))?;
    if start < min || end > max || start > end {
        anyhow::bail!("Range {}-{} out of bounds [{}, {}]", start, end, min, max);
    }
    Ok((start, end))
}

// ---------------------------------------------------------------------------
// ScheduledJob
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    pub schedule: CronSchedule,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub run_count: u64,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// In-memory cron-like task scheduler.
///
/// Maintains a set of scheduled jobs and checks on each `tick()` whether any
/// are due to run. The caller is responsible for invoking `tick()` periodically
/// (e.g. once per minute).
pub struct Scheduler {
    jobs: Vec<ScheduledJob>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self { jobs: Vec::new() }
    }

    /// Register a new job with a cron expression. Returns the job ID.
    pub fn add_job(
        &mut self,
        name: impl Into<String>,
        cron_expr: impl Into<String>,
    ) -> anyhow::Result<String> {
        let cron_expr = cron_expr.into();
        let schedule = CronSchedule::parse(&cron_expr)?;
        let job = ScheduledJob {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            cron_expr,
            schedule,
            enabled: true,
            last_run: None,
            next_run: None,
            run_count: 0,
        };
        let id = job.id.clone();
        self.jobs.push(job);
        Ok(id)
    }

    /// Remove a job by ID.
    pub fn remove_job(&mut self, id: &str) -> anyhow::Result<()> {
        let len_before = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        if self.jobs.len() == len_before {
            anyhow::bail!("Job not found: {}", id);
        }
        Ok(())
    }

    /// Enable a previously disabled job.
    pub fn enable_job(&mut self, id: &str) -> anyhow::Result<()> {
        let job = self
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", id))?;
        job.enabled = true;
        Ok(())
    }

    /// Disable a job so it will be skipped during `tick()`.
    pub fn disable_job(&mut self, id: &str) -> anyhow::Result<()> {
        let job = self
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", id))?;
        job.enabled = false;
        Ok(())
    }

    /// Returns all registered jobs.
    pub fn list_jobs(&self) -> &[ScheduledJob] {
        &self.jobs
    }

    /// Look up a job by ID.
    pub fn get_job(&self, id: &str) -> Option<&ScheduledJob> {
        self.jobs.iter().find(|j| j.id == id)
    }

    /// Check all enabled jobs against the provided time. Returns the IDs of
    /// jobs that matched and were marked as run.
    ///
    /// This is designed to be called once per minute. If the same minute is
    /// checked twice, jobs that already ran in that minute are skipped.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Vec<String> {
        let mut due = Vec::new();

        for job in &mut self.jobs {
            if !job.enabled {
                continue;
            }

            // Skip if we already ran during this same minute
            if let Some(last) = job.last_run {
                if last.format("%Y-%m-%d %H:%M").to_string()
                    == now.format("%Y-%m-%d %H:%M").to_string()
                {
                    continue;
                }
            }

            if job.schedule.matches(&now) {
                job.last_run = Some(now);
                job.run_count += 1;
                due.push(job.id.clone());
            }
        }

        due
    }

    /// Convenience wrapper: tick with `Utc::now()`.
    pub fn tick_now(&mut self) -> Vec<String> {
        self.tick(Utc::now())
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // -----------------------------------------------------------------------
    // Cron parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_every_minute() {
        let s = CronSchedule::parse("* * * * *").unwrap();
        assert_eq!(s.minutes.len(), 60); // 0..59
        assert_eq!(s.hours.len(), 24);
        assert_eq!(s.days_of_month.len(), 31);
        assert_eq!(s.months.len(), 12);
        assert_eq!(s.days_of_week.len(), 7);
    }

    #[test]
    fn parse_every_five_minutes() {
        let s = CronSchedule::parse("*/5 * * * *").unwrap();
        assert_eq!(s.minutes, vec![0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55]);
    }

    #[test]
    fn parse_specific_time() {
        let s = CronSchedule::parse("30 9 * * 1").unwrap();
        assert_eq!(s.minutes, vec![30]);
        assert_eq!(s.hours, vec![9]);
        assert_eq!(s.days_of_week, vec![1]); // Monday
    }

    #[test]
    fn parse_range() {
        let s = CronSchedule::parse("0 9-17 * * *").unwrap();
        assert_eq!(s.hours, vec![9, 10, 11, 12, 13, 14, 15, 16, 17]);
    }

    #[test]
    fn parse_list() {
        let s = CronSchedule::parse("0,15,30,45 * * * *").unwrap();
        assert_eq!(s.minutes, vec![0, 15, 30, 45]);
    }

    #[test]
    fn parse_range_with_step() {
        let s = CronSchedule::parse("1-10/3 * * * *").unwrap();
        assert_eq!(s.minutes, vec![1, 4, 7, 10]);
    }

    #[test]
    fn parse_invalid_field_count() {
        assert!(CronSchedule::parse("* * *").is_err());
        assert!(CronSchedule::parse("* * * * * *").is_err());
    }

    #[test]
    fn parse_out_of_range() {
        assert!(CronSchedule::parse("60 * * * *").is_err());
        assert!(CronSchedule::parse("* 25 * * *").is_err());
        assert!(CronSchedule::parse("* * 0 * *").is_err()); // day-of-month min is 1
        assert!(CronSchedule::parse("* * * 13 *").is_err());
        assert!(CronSchedule::parse("* * * * 7").is_err());
    }

    #[test]
    fn parse_zero_step_errors() {
        assert!(CronSchedule::parse("*/0 * * * *").is_err());
    }

    #[test]
    fn cron_matches_datetime() {
        // 2025-01-06 is a Monday (weekday 1)
        let dt = Utc.with_ymd_and_hms(2025, 1, 6, 9, 30, 0).unwrap();

        // "30 9 * * 1" = 9:30 on Mondays
        let s = CronSchedule::parse("30 9 * * 1").unwrap();
        assert!(s.matches(&dt));

        // Should not match at 9:31
        let dt2 = Utc.with_ymd_and_hms(2025, 1, 6, 9, 31, 0).unwrap();
        assert!(!s.matches(&dt2));

        // Should not match on Tuesday
        let dt3 = Utc.with_ymd_and_hms(2025, 1, 7, 9, 30, 0).unwrap();
        assert!(!s.matches(&dt3));
    }

    #[test]
    fn cron_serde_roundtrip() {
        let s = CronSchedule::parse("*/10 9-17 * * 1-5").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let parsed: CronSchedule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, s);
    }

    // -----------------------------------------------------------------------
    // Scheduler lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn add_and_list_jobs() {
        let mut scheduler = Scheduler::new();
        let id = scheduler.add_job("test-job", "*/5 * * * *").unwrap();
        assert!(!id.is_empty());

        let jobs = scheduler.list_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "test-job");
        assert_eq!(jobs[0].cron_expr, "*/5 * * * *");
        assert!(jobs[0].enabled);
        assert_eq!(jobs[0].run_count, 0);
    }

    #[test]
    fn add_job_with_invalid_cron_errors() {
        let mut scheduler = Scheduler::new();
        assert!(scheduler.add_job("bad", "not a cron").is_err());
    }

    #[test]
    fn remove_job() {
        let mut scheduler = Scheduler::new();
        let id = scheduler.add_job("remove-me", "* * * * *").unwrap();
        assert_eq!(scheduler.list_jobs().len(), 1);

        scheduler.remove_job(&id).unwrap();
        assert!(scheduler.list_jobs().is_empty());
    }

    #[test]
    fn remove_nonexistent_job_errors() {
        let mut scheduler = Scheduler::new();
        assert!(scheduler.remove_job("no-such-id").is_err());
    }

    #[test]
    fn enable_disable_job() {
        let mut scheduler = Scheduler::new();
        let id = scheduler.add_job("toggle", "* * * * *").unwrap();

        assert!(scheduler.get_job(&id).unwrap().enabled);

        scheduler.disable_job(&id).unwrap();
        assert!(!scheduler.get_job(&id).unwrap().enabled);

        scheduler.enable_job(&id).unwrap();
        assert!(scheduler.get_job(&id).unwrap().enabled);
    }

    #[test]
    fn enable_nonexistent_errors() {
        let mut scheduler = Scheduler::new();
        assert!(scheduler.enable_job("nope").is_err());
        assert!(scheduler.disable_job("nope").is_err());
    }

    #[test]
    fn tick_fires_matching_job() {
        let mut scheduler = Scheduler::new();
        // "30 9 * * *" = every day at 9:30
        let id = scheduler.add_job("nine-thirty", "30 9 * * *").unwrap();

        let at_930 = Utc.with_ymd_and_hms(2025, 6, 15, 9, 30, 0).unwrap();
        let due = scheduler.tick(at_930);

        assert_eq!(due.len(), 1);
        assert_eq!(due[0], id);
        assert_eq!(scheduler.get_job(&id).unwrap().run_count, 1);
        assert!(scheduler.get_job(&id).unwrap().last_run.is_some());
    }

    #[test]
    fn tick_skips_disabled_job() {
        let mut scheduler = Scheduler::new();
        let id = scheduler.add_job("disabled", "* * * * *").unwrap();
        scheduler.disable_job(&id).unwrap();

        let now = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let due = scheduler.tick(now);
        assert!(due.is_empty());
        assert_eq!(scheduler.get_job(&id).unwrap().run_count, 0);
    }

    #[test]
    fn tick_does_not_double_fire_same_minute() {
        let mut scheduler = Scheduler::new();
        let id = scheduler.add_job("once-per-min", "* * * * *").unwrap();

        let now = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let due1 = scheduler.tick(now);
        assert_eq!(due1.len(), 1);

        // Tick again at same minute, different second
        let same_min = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 30).unwrap();
        let due2 = scheduler.tick(same_min);
        assert!(due2.is_empty());
        assert_eq!(scheduler.get_job(&id).unwrap().run_count, 1);
    }

    #[test]
    fn tick_fires_again_next_minute() {
        let mut scheduler = Scheduler::new();
        let id = scheduler.add_job("every-min", "* * * * *").unwrap();

        let min0 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        scheduler.tick(min0);
        assert_eq!(scheduler.get_job(&id).unwrap().run_count, 1);

        let min1 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 1, 0).unwrap();
        let due = scheduler.tick(min1);
        assert_eq!(due.len(), 1);
        assert_eq!(scheduler.get_job(&id).unwrap().run_count, 2);
    }

    #[test]
    fn tick_non_matching_minute_skips() {
        let mut scheduler = Scheduler::new();
        // Only at minute 0
        scheduler.add_job("hourly", "0 * * * *").unwrap();

        let at_15 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 15, 0).unwrap();
        let due = scheduler.tick(at_15);
        assert!(due.is_empty());
    }

    #[test]
    fn multiple_jobs_tick() {
        let mut scheduler = Scheduler::new();
        let id1 = scheduler.add_job("every-min", "* * * * *").unwrap();
        let id2 = scheduler.add_job("hourly", "0 * * * *").unwrap();

        // At minute 0, both should fire
        let at_0 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let due = scheduler.tick(at_0);
        assert_eq!(due.len(), 2);
        assert!(due.contains(&id1));
        assert!(due.contains(&id2));

        // At minute 5, only every-min should fire
        let at_5 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 5, 0).unwrap();
        let due = scheduler.tick(at_5);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0], id1);
    }

    #[test]
    fn scheduler_default() {
        let scheduler = Scheduler::default();
        assert!(scheduler.list_jobs().is_empty());
    }

    #[test]
    fn get_job_returns_none_for_unknown() {
        let scheduler = Scheduler::new();
        assert!(scheduler.get_job("unknown").is_none());
    }

    #[test]
    fn job_serde_roundtrip() {
        let mut scheduler = Scheduler::new();
        let id = scheduler.add_job("serde-test", "*/15 9-17 * * 1-5").unwrap();

        let job = scheduler.get_job(&id).unwrap();
        let json = serde_json::to_string(job).unwrap();
        let parsed: ScheduledJob = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, job.id);
        assert_eq!(parsed.name, "serde-test");
        assert_eq!(parsed.cron_expr, "*/15 9-17 * * 1-5");
        assert!(parsed.enabled);
    }
}
