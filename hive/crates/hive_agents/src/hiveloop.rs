//! HiveLoop — autonomous iteration with pause/resume, checkpointing,
//! cost limits, and completion detection.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Loop State
// ---------------------------------------------------------------------------

/// Status of an autonomous loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopStatus {
    Running,
    Paused,
    Completed,
    CostLimitReached,
    TimeLimitReached,
    IterationLimitReached,
    Failed,
}

/// Configuration for an autonomous loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    pub max_iterations: usize,
    pub cost_limit_usd: f64,
    pub time_limit_secs: u64,
    pub completion_phrases: Vec<String>,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            cost_limit_usd: 2.0,
            time_limit_secs: 600,
            completion_phrases: vec![
                "task complete".into(),
                "all done".into(),
                "finished".into(),
                "implementation complete".into(),
            ],
        }
    }
}

/// Checkpoint for saving/restoring loop state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub iteration: usize,
    pub total_cost: f64,
    pub last_output: String,
    pub context_messages: usize,
}

/// Tracks the state of an autonomous iteration loop.
pub struct HiveLoop {
    pub config: LoopConfig,
    pub status: LoopStatus,
    pub iteration: usize,
    pub total_cost: f64,
    pub last_output: String,
    started_at: Option<Instant>,
}

impl HiveLoop {
    pub fn new(config: LoopConfig) -> Self {
        Self {
            config,
            status: LoopStatus::Running,
            iteration: 0,
            total_cost: 0.0,
            last_output: String::new(),
            started_at: None,
        }
    }

    /// Start the loop timer.
    pub fn start(&mut self) {
        self.started_at = Some(Instant::now());
        self.status = LoopStatus::Running;
    }

    /// Check if the loop should continue.
    pub fn should_continue(&self) -> bool {
        if self.status != LoopStatus::Running {
            return false;
        }
        if self.iteration >= self.config.max_iterations {
            return false;
        }
        if self.total_cost >= self.config.cost_limit_usd {
            return false;
        }
        if let Some(started) = self.started_at {
            if started.elapsed() >= Duration::from_secs(self.config.time_limit_secs) {
                return false;
            }
        }
        true
    }

    /// Record a completed iteration and check limits.
    pub fn record_iteration(&mut self, output: &str, cost: f64) -> LoopStatus {
        self.iteration += 1;
        self.total_cost += cost;
        self.last_output = output.to_string();

        // Check completion phrases
        let lower = output.to_lowercase();
        for phrase in &self.config.completion_phrases {
            if lower.contains(&phrase.to_lowercase()) {
                self.status = LoopStatus::Completed;
                return self.status;
            }
        }

        // Check limits
        if self.iteration >= self.config.max_iterations {
            self.status = LoopStatus::IterationLimitReached;
        } else if self.total_cost >= self.config.cost_limit_usd {
            self.status = LoopStatus::CostLimitReached;
        } else if let Some(started) = self.started_at {
            if started.elapsed() >= Duration::from_secs(self.config.time_limit_secs) {
                self.status = LoopStatus::TimeLimitReached;
            }
        }

        self.status
    }

    /// Pause the loop.
    pub fn pause(&mut self) {
        if self.status == LoopStatus::Running {
            self.status = LoopStatus::Paused;
        }
    }

    /// Resume the loop.
    pub fn resume(&mut self) {
        if self.status == LoopStatus::Paused {
            self.status = LoopStatus::Running;
        }
    }

    /// Create a checkpoint for persistence.
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            iteration: self.iteration,
            total_cost: self.total_cost,
            last_output: self.last_output.clone(),
            context_messages: 0,
        }
    }

    /// Restore from a checkpoint.
    pub fn restore(&mut self, checkpoint: &Checkpoint) {
        self.iteration = checkpoint.iteration;
        self.total_cost = checkpoint.total_cost;
        self.last_output = checkpoint.last_output.clone();
        self.status = LoopStatus::Paused;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_loop() {
        let mut hive_loop = HiveLoop::new(LoopConfig::default());
        hive_loop.start();
        assert!(hive_loop.should_continue());
        assert_eq!(hive_loop.iteration, 0);
    }

    #[test]
    fn iteration_tracking() {
        let mut hive_loop = HiveLoop::new(LoopConfig {
            max_iterations: 3,
            ..Default::default()
        });
        hive_loop.start();

        assert_eq!(hive_loop.record_iteration("step 1", 0.01), LoopStatus::Running);
        assert_eq!(hive_loop.record_iteration("step 2", 0.01), LoopStatus::Running);
        assert_eq!(hive_loop.record_iteration("step 3", 0.01), LoopStatus::IterationLimitReached);
        assert!(!hive_loop.should_continue());
    }

    #[test]
    fn completion_detection() {
        let mut hive_loop = HiveLoop::new(LoopConfig::default());
        hive_loop.start();
        let status = hive_loop.record_iteration("I have finished the task. Task complete.", 0.01);
        assert_eq!(status, LoopStatus::Completed);
    }

    #[test]
    fn cost_limit() {
        let mut hive_loop = HiveLoop::new(LoopConfig {
            cost_limit_usd: 0.05,
            ..Default::default()
        });
        hive_loop.start();
        hive_loop.record_iteration("step 1", 0.03);
        let status = hive_loop.record_iteration("step 2", 0.03);
        assert_eq!(status, LoopStatus::CostLimitReached);
    }

    #[test]
    fn pause_resume() {
        let mut hive_loop = HiveLoop::new(LoopConfig::default());
        hive_loop.start();
        assert!(hive_loop.should_continue());

        hive_loop.pause();
        assert!(!hive_loop.should_continue());

        hive_loop.resume();
        assert!(hive_loop.should_continue());
    }

    #[test]
    fn checkpoint_restore() {
        let mut hive_loop = HiveLoop::new(LoopConfig::default());
        hive_loop.start();
        hive_loop.record_iteration("first", 0.05);
        hive_loop.record_iteration("second", 0.03);

        let cp = hive_loop.checkpoint();
        assert_eq!(cp.iteration, 2);
        assert!((cp.total_cost - 0.08).abs() < 0.001);

        let mut new_loop = HiveLoop::new(LoopConfig::default());
        new_loop.restore(&cp);
        assert_eq!(new_loop.iteration, 2);
        assert_eq!(new_loop.status, LoopStatus::Paused);
    }
}
