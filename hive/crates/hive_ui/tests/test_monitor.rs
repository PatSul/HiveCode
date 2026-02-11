use chrono::Utc;
use hive_ui::panels::monitor::*;

#[test]
fn monitor_data_empty_returns_idle_state() {
    let data = MonitorData::empty();
    assert_eq!(data.status, AgentSystemStatus::Idle);
    assert!(data.active_agents.is_empty());
    assert_eq!(data.completed_tasks, 0);
    assert_eq!(data.total_runs, 0);
    assert!(data.current_run_id.is_none());
    assert!(data.run_history.is_empty());
    assert_eq!(data.request_queue_length, 0);
    assert_eq!(data.active_streams, 0);
    assert_eq!(data.uptime_secs, 0);
    assert!(data.providers.is_empty());
}

#[test]
fn monitor_data_sample_has_populated_fields() {
    let data = MonitorData::sample();
    assert_eq!(data.status, AgentSystemStatus::Running);
    assert!(!data.active_agents.is_empty());
    assert!(data.completed_tasks > 0);
    assert!(data.total_runs > 0);
    assert!(data.current_run_id.is_some());
    assert!(!data.run_history.is_empty());
    assert!(!data.providers.is_empty());
    assert!(data.active_streams > 0);
    assert!(data.uptime_secs > 0);
}

#[test]
fn agent_system_status_labels_are_correct() {
    assert_eq!(AgentSystemStatus::Idle.label(), "Idle");
    assert_eq!(AgentSystemStatus::Running.label(), "Running");
    assert_eq!(AgentSystemStatus::Paused.label(), "Paused");
    assert_eq!(AgentSystemStatus::Error.label(), "Error");
}

#[test]
fn agent_status_labels_are_correct() {
    assert_eq!(AgentStatus::Idle.label(), "Idle");
    assert_eq!(AgentStatus::Working.label(), "Working");
    assert_eq!(AgentStatus::Waiting.label(), "Waiting");
    assert_eq!(AgentStatus::Done.label(), "Done");
    assert_eq!(AgentStatus::Failed.label(), "Failed");
}

#[test]
fn active_agent_new_sets_all_fields() {
    let now = Utc::now();
    let agent = ActiveAgent::new("Coder", AgentStatus::Working, "Writing code", "claude-sonnet-4-5", now);
    assert_eq!(agent.role, "Coder");
    assert_eq!(agent.status, AgentStatus::Working);
    assert_eq!(agent.phase, "Writing code");
    assert_eq!(agent.model, "claude-sonnet-4-5");
    assert_eq!(agent.started_at, now);
}

#[test]
fn agent_role_all_returns_nine_roles() {
    let roles = AgentRole::all();
    assert_eq!(roles.len(), 9);
    assert_eq!(roles[0], AgentRole::Architect);
    assert_eq!(roles[8], AgentRole::TaskVerifier);
}

#[test]
fn agent_role_labels_are_nonempty() {
    for role in AgentRole::all() {
        assert!(!role.label().is_empty());
    }
}

#[test]
fn fmt_duration_formats_correctly() {
    assert_eq!(MonitorPanel::fmt_duration(0), "Running");
    assert_eq!(MonitorPanel::fmt_duration(45), "45s");
    assert_eq!(MonitorPanel::fmt_duration(90), "1m 30s");
    assert_eq!(MonitorPanel::fmt_duration(3600), "60m 0s");
}

#[test]
fn run_history_entry_construction() {
    let entry = RunHistoryEntry::new(
        "run-001", "Test task", 3, AgentSystemStatus::Idle, 1.50, "10:00", 120,
    );
    assert_eq!(entry.id, "run-001");
    assert_eq!(entry.task_summary, "Test task");
    assert_eq!(entry.agents_used, 3);
    assert_eq!(entry.status, AgentSystemStatus::Idle);
    assert!((entry.cost - 1.50).abs() < f64::EPSILON);
    assert_eq!(entry.started_at, "10:00");
    assert_eq!(entry.duration_secs, 120);
}

// -- New system resource tests --

#[test]
fn system_resources_placeholder_has_sane_values() {
    let res = SystemResources::placeholder();
    assert!(res.cpu_percent > 0.0 && res.cpu_percent <= 100.0);
    assert!(res.memory_used > 0);
    assert!(res.memory_total > res.memory_used);
    assert!(res.disk_used > 0);
    assert!(res.disk_total > res.disk_used);
}

#[test]
fn memory_percent_calculation() {
    let res = SystemResources {
        cpu_percent: 50.0,
        memory_used: 500,
        memory_total: 1000,
        disk_used: 0,
        disk_total: 0,
    };
    assert!((res.memory_percent() - 50.0).abs() < f64::EPSILON);
}

#[test]
fn disk_percent_calculation() {
    let res = SystemResources {
        cpu_percent: 0.0,
        memory_used: 0,
        memory_total: 0,
        disk_used: 250,
        disk_total: 1000,
    };
    assert!((res.disk_percent() - 25.0).abs() < f64::EPSILON);
}

#[test]
fn memory_percent_zero_total_returns_zero() {
    let res = SystemResources {
        cpu_percent: 0.0,
        memory_used: 100,
        memory_total: 0,
        disk_used: 0,
        disk_total: 0,
    };
    assert!((res.memory_percent() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn disk_percent_zero_total_returns_zero() {
    let res = SystemResources {
        cpu_percent: 0.0,
        memory_used: 0,
        memory_total: 0,
        disk_used: 100,
        disk_total: 0,
    };
    assert!((res.disk_percent() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn provider_status_online_count() {
    let data = MonitorData::sample();
    let online = data.online_provider_count();
    let total = data.providers.len();
    assert!(online > 0);
    assert!(online <= total);
}

#[test]
fn fmt_uptime_formats_correctly() {
    assert_eq!(MonitorPanel::fmt_uptime(0), "0s");
    assert_eq!(MonitorPanel::fmt_uptime(45), "45s");
    assert_eq!(MonitorPanel::fmt_uptime(90), "1m 30s");
    assert_eq!(MonitorPanel::fmt_uptime(3661), "1h 1m 1s");
    assert_eq!(MonitorPanel::fmt_uptime(7200), "2h 0m 0s");
}

#[test]
fn fmt_bytes_formats_correctly() {
    assert_eq!(MonitorPanel::fmt_bytes(0), "0 B");
    assert_eq!(MonitorPanel::fmt_bytes(512), "512 B");
    assert_eq!(MonitorPanel::fmt_bytes(1024), "1 KB");
    assert_eq!(MonitorPanel::fmt_bytes(1_048_576), "1.0 MB");
    assert_eq!(MonitorPanel::fmt_bytes(1_073_741_824), "1.0 GB");
    assert_eq!(MonitorPanel::fmt_bytes(1_099_511_627_776), "1.0 TB");
}

#[test]
fn provider_status_new_sets_fields() {
    let p = ProviderStatus::new("TestProvider", true, Some(42));
    assert_eq!(p.name, "TestProvider");
    assert!(p.online);
    assert_eq!(p.latency_ms, Some(42));

    let p2 = ProviderStatus::new("Offline", false, None);
    assert!(!p2.online);
    assert_eq!(p2.latency_ms, None);
}
