use hive_ui::panels::settings::*;

// ---------------------------------------------------------------------------
// SettingsData -- from_config & defaults
// ---------------------------------------------------------------------------

#[test]
fn test_settings_data_default() {
    let d = SettingsData::default();
    assert!(!d.has_anthropic_key);
    assert!(!d.has_openai_key);
    assert!(!d.has_openrouter_key);
    assert!(!d.has_google_key);
    assert_eq!(d.ollama_url, "http://localhost:11434");
    assert_eq!(d.lmstudio_url, "http://localhost:1234");
    assert!(d.local_provider_url.is_none());
    assert!(!d.privacy_mode);
    assert!(d.auto_routing);
    assert_eq!(d.daily_budget_usd, 10.0);
    assert_eq!(d.monthly_budget_usd, 100.0);
    assert!(d.auto_update);
    assert!(d.notifications_enabled);
}

#[test]
fn test_settings_data_from_config_defaults() {
    let cfg = hive_core::HiveConfig::default();
    let d = SettingsData::from_config(&cfg);
    assert!(!d.has_anthropic_key);
    assert!(!d.has_openai_key);
    assert!(!d.has_openrouter_key);
    assert!(!d.has_google_key);
    assert!(!d.has_any_cloud_key());
    assert_eq!(d.configured_key_count(), 0);
}

#[test]
fn test_settings_data_from_config_with_keys() {
    let mut cfg = hive_core::HiveConfig::default();
    cfg.anthropic_api_key = Some("sk-ant-test-key".to_string());
    cfg.openai_api_key = Some("sk-openai-test".to_string());

    let d = SettingsData::from_config(&cfg);
    assert!(d.has_anthropic_key);
    assert!(d.has_openai_key);
    assert!(!d.has_openrouter_key);
    assert!(!d.has_google_key);
    assert!(d.has_any_cloud_key());
    assert_eq!(d.configured_key_count(), 2);
}

#[test]
fn test_settings_data_empty_key_not_counted() {
    let mut cfg = hive_core::HiveConfig::default();
    cfg.anthropic_api_key = Some(String::new()); // empty string
    cfg.openai_api_key = None;

    let d = SettingsData::from_config(&cfg);
    assert!(!d.has_anthropic_key);
    assert!(!d.has_openai_key);
    assert_eq!(d.configured_key_count(), 0);
    assert!(!d.has_any_cloud_key());
}

#[test]
fn test_settings_data_all_four_keys() {
    let mut cfg = hive_core::HiveConfig::default();
    cfg.anthropic_api_key = Some("key1".to_string());
    cfg.openai_api_key = Some("key2".to_string());
    cfg.openrouter_api_key = Some("key3".to_string());
    cfg.google_api_key = Some("key4".to_string());

    let d = SettingsData::from_config(&cfg);
    assert_eq!(d.configured_key_count(), 4);
}

#[test]
fn test_settings_data_from_trait() {
    let cfg = hive_core::HiveConfig::default();
    let d: SettingsData = (&cfg).into();
    assert!(!d.has_any_cloud_key());
}

#[test]
fn test_settings_data_preserves_urls() {
    let mut cfg = hive_core::HiveConfig::default();
    cfg.ollama_url = "http://my-server:11434".to_string();
    cfg.lmstudio_url = "http://my-server:1234".to_string();
    cfg.local_provider_url = Some("http://custom:8080".to_string());

    let d = SettingsData::from_config(&cfg);
    assert_eq!(d.ollama_url, "http://my-server:11434");
    assert_eq!(d.lmstudio_url, "http://my-server:1234");
    assert_eq!(d.local_provider_url.as_deref(), Some("http://custom:8080"));
}

#[test]
fn test_settings_data_preserves_toggles() {
    let mut cfg = hive_core::HiveConfig::default();
    cfg.privacy_mode = true;
    cfg.auto_routing = false;
    cfg.auto_update = false;
    cfg.notifications_enabled = false;

    let d = SettingsData::from_config(&cfg);
    assert!(d.privacy_mode);
    assert!(!d.auto_routing);
    assert!(!d.auto_update);
    assert!(!d.notifications_enabled);
}

#[test]
fn test_settings_data_preserves_budget() {
    let mut cfg = hive_core::HiveConfig::default();
    cfg.daily_budget_usd = 25.50;
    cfg.monthly_budget_usd = 500.0;

    let d = SettingsData::from_config(&cfg);
    assert_eq!(d.daily_budget_usd, 25.50);
    assert_eq!(d.monthly_budget_usd, 500.0);
}

#[test]
fn test_settings_data_clone() {
    let d = SettingsData::default();
    let d2 = d.clone();
    assert_eq!(d.has_anthropic_key, d2.has_anthropic_key);
    assert_eq!(d.ollama_url, d2.ollama_url);
    assert_eq!(d.privacy_mode, d2.privacy_mode);
}

#[test]
fn test_settings_data_debug() {
    let d = SettingsData::default();
    let debug_str = format!("{:?}", d);
    assert!(debug_str.contains("SettingsData"));
}

// ---------------------------------------------------------------------------
// TTS settings
// ---------------------------------------------------------------------------

#[test]
fn test_settings_data_tts_defaults() {
    let d = SettingsData::default();
    assert!(!d.has_elevenlabs_key);
    assert!(!d.has_telnyx_key);
    assert!(!d.tts_enabled);
    assert!(!d.tts_auto_speak);
    assert_eq!(d.tts_provider, "qwen3");
    assert!((d.tts_speed - 1.0).abs() < f32::EPSILON);
    assert!(!d.clawdtalk_enabled);
}

#[test]
fn test_settings_data_tts_from_config() {
    let mut cfg = hive_core::HiveConfig::default();
    cfg.elevenlabs_api_key = Some("el-key".to_string());
    cfg.telnyx_api_key = Some("tx-key".to_string());
    cfg.tts_enabled = true;
    cfg.tts_auto_speak = true;
    cfg.tts_provider = "elevenlabs".to_string();
    cfg.tts_speed = 1.5;
    cfg.clawdtalk_enabled = true;

    let d = SettingsData::from_config(&cfg);
    assert!(d.has_elevenlabs_key);
    assert!(d.has_telnyx_key);
    assert!(d.tts_enabled);
    assert!(d.tts_auto_speak);
    assert_eq!(d.tts_provider, "elevenlabs");
    assert!((d.tts_speed - 1.5).abs() < f32::EPSILON);
    assert!(d.clawdtalk_enabled);
}

// ---------------------------------------------------------------------------
// SettingsSaved event
// ---------------------------------------------------------------------------

#[test]
fn test_settings_saved_event_is_clone() {
    let e = SettingsSaved;
    let e2 = e.clone();
    let _ = format!("{:?}", e2);
}
