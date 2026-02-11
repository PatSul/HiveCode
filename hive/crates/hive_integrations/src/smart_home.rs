use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DISCOVERY_URL: &str = "https://discovery.meethue.com";

/// A discovered Philips Hue bridge on the local network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HueBridge {
    pub id: String,
    #[serde(alias = "internalipaddress")]
    pub ip: String,
}

/// A Philips Hue light with its current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HueLight {
    pub id: String,
    pub name: String,
    pub on: bool,
    pub brightness: u8,
    pub reachable: bool,
}

/// A Philips Hue scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HueScene {
    pub id: String,
    pub name: String,
}

/// Client for the Philips Hue REST API.
///
/// Communicates with a Hue bridge on the local network to control
/// lights, scenes, and query bridge state.
#[allow(dead_code)]
pub struct PhilipsHueClient {
    client: Client,
    bridge_ip: String,
    api_key: String,
    base_url: String,
}

impl PhilipsHueClient {
    /// Create a new client targeting the given bridge IP and API key.
    pub fn new(bridge_ip: &str, api_key: &str) -> Self {
        let base_url = format!("http://{}/api/{}", bridge_ip, api_key);
        Self {
            client: Client::new(),
            bridge_ip: bridge_ip.to_string(),
            api_key: api_key.to_string(),
            base_url,
        }
    }

    /// Create a new client with a fully custom base URL (useful for tests).
    pub fn with_base_url(bridge_ip: &str, api_key: &str, base_url: &str) -> Self {
        Self {
            client: Client::new(),
            bridge_ip: bridge_ip.to_string(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Discover Hue bridges on the local network via the Philips discovery service.
    pub async fn discover_bridges() -> Result<Vec<HueBridge>> {
        debug!("discovering Hue bridges via {}", DISCOVERY_URL);

        let client = Client::new();
        let response = client
            .get(DISCOVERY_URL)
            .send()
            .await
            .context("Hue bridge discovery request failed")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Hue discovery error ({})", status);
        }

        let bridges: Vec<HueBridge> = response
            .json()
            .await
            .context("failed to parse Hue discovery response")?;

        Ok(bridges)
    }

    /// List all lights connected to the bridge.
    pub async fn list_lights(&self) -> Result<Vec<HueLight>> {
        let url = format!("{}/lights", self.base_url);
        debug!(url = %url, "listing Hue lights");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Hue list lights request failed")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to parse Hue lights response")?;

        if !status.is_success() {
            anyhow::bail!("Hue API error ({}): {}", status, body);
        }

        // The Hue API returns lights as a map of ID -> light object.
        let mut lights = Vec::new();
        if let Some(obj) = body.as_object() {
            for (id, val) in obj {
                let name = val["name"].as_str().unwrap_or("").to_string();
                let on = val["state"]["on"].as_bool().unwrap_or(false);
                let brightness = val["state"]["bri"].as_u64().unwrap_or(0) as u8;
                let reachable = val["state"]["reachable"].as_bool().unwrap_or(false);

                lights.push(HueLight {
                    id: id.clone(),
                    name,
                    on,
                    brightness,
                    reachable,
                });
            }
        }

        Ok(lights)
    }

    /// Set the on/off state and optional brightness of a light.
    ///
    /// `brightness` is a value from 1 to 254 (Hue scale).
    pub async fn set_light_state(
        &self,
        light_id: &str,
        on: bool,
        brightness: Option<u8>,
    ) -> Result<()> {
        let url = format!("{}/lights/{}/state", self.base_url, light_id);

        let mut payload = serde_json::json!({ "on": on });
        if let Some(bri) = brightness {
            payload["bri"] = serde_json::json!(bri);
        }

        debug!(url = %url, on = on, "setting Hue light state");

        let response = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .await
            .context("Hue set light state request failed")?;

        let status = response.status();
        if !status.is_success() {
            let err_body: serde_json::Value = response
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "unknown"}));
            anyhow::bail!("Hue set state error ({}): {}", status, err_body);
        }

        Ok(())
    }

    /// Set the color of a light using hue and saturation values.
    ///
    /// `hue` ranges from 0 to 65535, `saturation` from 0 to 254.
    pub async fn set_light_color(
        &self,
        light_id: &str,
        hue: u16,
        saturation: u8,
    ) -> Result<()> {
        let url = format!("{}/lights/{}/state", self.base_url, light_id);
        let payload = serde_json::json!({ "hue": hue, "sat": saturation });

        debug!(url = %url, hue = hue, sat = saturation, "setting Hue light color");

        let response = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .await
            .context("Hue set light color request failed")?;

        let status = response.status();
        if !status.is_success() {
            let err_body: serde_json::Value = response
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "unknown"}));
            anyhow::bail!("Hue set color error ({}): {}", status, err_body);
        }

        Ok(())
    }

    /// List all scenes stored on the bridge.
    pub async fn list_scenes(&self) -> Result<Vec<HueScene>> {
        let url = format!("{}/scenes", self.base_url);
        debug!(url = %url, "listing Hue scenes");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Hue list scenes request failed")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to parse Hue scenes response")?;

        if !status.is_success() {
            anyhow::bail!("Hue API error ({}): {}", status, body);
        }

        let mut scenes = Vec::new();
        if let Some(obj) = body.as_object() {
            for (id, val) in obj {
                let name = val["name"].as_str().unwrap_or("").to_string();
                scenes.push(HueScene {
                    id: id.clone(),
                    name,
                });
            }
        }

        Ok(scenes)
    }

    /// Activate a scene by ID.
    pub async fn activate_scene(&self, scene_id: &str) -> Result<()> {
        let url = format!("{}/groups/0/action", self.base_url);
        let payload = serde_json::json!({ "scene": scene_id });

        debug!(url = %url, scene_id = %scene_id, "activating Hue scene");

        let response = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .await
            .context("Hue activate scene request failed")?;

        let status = response.status();
        if !status.is_success() {
            let err_body: serde_json::Value = response
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": "unknown"}));
            anyhow::bail!("Hue activate scene error ({}): {}", status, err_body);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_client_default_base_url() {
        let client = PhilipsHueClient::new("192.168.1.100", "abc123key");
        assert_eq!(client.base_url(), "http://192.168.1.100/api/abc123key");
    }

    #[test]
    fn test_client_custom_base_url() {
        let client =
            PhilipsHueClient::with_base_url("192.168.1.100", "key", "http://localhost:8080/");
        assert_eq!(client.base_url(), "http://localhost:8080");
    }

    #[test]
    fn test_hue_bridge_serde_roundtrip() {
        let bridge = HueBridge {
            id: "001788fffe123456".into(),
            ip: "192.168.1.100".into(),
        };
        let json = serde_json::to_string(&bridge).unwrap();
        let parsed: HueBridge = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "001788fffe123456");
        assert_eq!(parsed.ip, "192.168.1.100");
    }

    #[test]
    fn test_hue_bridge_deserialize_discovery_format() {
        // The discovery API returns "internalipaddress" not "ip".
        let json = r#"{"id":"001788fffe123456","internalipaddress":"192.168.1.50"}"#;
        let bridge: HueBridge = serde_json::from_str(json).unwrap();
        assert_eq!(bridge.id, "001788fffe123456");
        assert_eq!(bridge.ip, "192.168.1.50");
    }

    #[test]
    fn test_hue_light_serde_roundtrip() {
        let light = HueLight {
            id: "1".into(),
            name: "Living room".into(),
            on: true,
            brightness: 200,
            reachable: true,
        };
        let json = serde_json::to_string(&light).unwrap();
        let parsed: HueLight = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "1");
        assert_eq!(parsed.name, "Living room");
        assert!(parsed.on);
        assert_eq!(parsed.brightness, 200);
        assert!(parsed.reachable);
    }

    #[test]
    fn test_hue_scene_serde_roundtrip() {
        let scene = HueScene {
            id: "scene-abc".into(),
            name: "Movie night".into(),
        };
        let json = serde_json::to_string(&scene).unwrap();
        let parsed: HueScene = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "scene-abc");
        assert_eq!(parsed.name, "Movie night");
    }

    #[test]
    fn test_lights_url_construction() {
        let client = PhilipsHueClient::new("192.168.1.100", "testkey");
        let url = build_url(client.base_url(), "/lights");
        assert_eq!(url, "http://192.168.1.100/api/testkey/lights");
    }

    #[test]
    fn test_light_state_url_construction() {
        let client = PhilipsHueClient::new("192.168.1.100", "testkey");
        let url = build_url(client.base_url(), "/lights/3/state");
        assert_eq!(url, "http://192.168.1.100/api/testkey/lights/3/state");
    }

    #[test]
    fn test_scenes_url_construction() {
        let client = PhilipsHueClient::new("192.168.1.100", "testkey");
        let url = build_url(client.base_url(), "/scenes");
        assert_eq!(url, "http://192.168.1.100/api/testkey/scenes");
    }

    #[test]
    fn test_activate_scene_payload() {
        let payload = serde_json::json!({ "scene": "my-scene-id" });
        assert_eq!(payload["scene"], "my-scene-id");
    }

    #[test]
    fn test_set_state_payload_with_brightness() {
        let mut payload = serde_json::json!({ "on": true });
        payload["bri"] = serde_json::json!(128u8);
        assert_eq!(payload["on"], true);
        assert_eq!(payload["bri"], 128);
    }

    #[test]
    fn test_set_color_payload() {
        let payload = serde_json::json!({ "hue": 25000u16, "sat": 200u8 });
        assert_eq!(payload["hue"], 25000);
        assert_eq!(payload["sat"], 200);
    }
}
