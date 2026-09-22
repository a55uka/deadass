use deadass_shared::{OpenShockConfig, ShockConfig, ShockKind};
use std::time::Duration;

pub struct OpenShockClient {
    http: reqwest::Client,
}

const MIN_INTENSITY: u8 = 1;
const MAX_INTENSITY: u8 = 100;
const MIN_DURATION_MS: u64 = 300;
const MAX_DURATION_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy)]
pub struct ControlCommand {
    pub kind: ShockKind,
    pub intensity: u8,
    pub duration_ms: u64,
}

impl ControlCommand {
    pub fn from_shock(config: &ShockConfig) -> Option<Self> {
        let config = config.clamped();
        if !config.enabled {
            return None;
        }
        Some(Self {
            kind: config.kind,
            intensity: config.intensity,
            duration_ms: config.duration_ms,
        })
    }

    fn intensity(self) -> u8 {
        self.intensity.clamp(MIN_INTENSITY, MAX_INTENSITY)
    }

    fn duration_ms(self) -> u64 {
        self.duration_ms.clamp(MIN_DURATION_MS, MAX_DURATION_MS)
    }

    pub fn kind_str(self) -> &'static str {
        match self.kind {
            ShockKind::Shock => "shock",
            ShockKind::Vibrate => "vibrate",
            ShockKind::Sound => "sound",
        }
    }
}

impl OpenShockClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("deadass/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest client builds"),
        }
    }

    pub fn payload(command: ControlCommand, shocker_ids: &[String]) -> serde_json::Value {
        serde_json::json!({
            "shocks": shocker_ids.iter().map(|id| serde_json::json!({
                "id": id,
                "type": command.kind_str(),
                "intensity": command.intensity(),
                "duration": command.duration_ms(),
            })).collect::<Vec<_>>(),
            "custom": false,
        })
    }

    fn endpoint(config: &OpenShockConfig) -> String {
        let base = config.base_url.trim_end_matches('/');
        format!("{base}/2/shockers/control")
    }

    pub fn configured(config: &OpenShockConfig) -> bool {
        config.enabled
            && !config.api_token.trim().is_empty()
            && config
                .shockers
                .iter()
                .any(|shocker| !shocker.id.trim().is_empty())
    }

    pub async fn control(
        &self,
        config: &OpenShockConfig,
        command: ControlCommand,
        shocker_ids: &[String],
    ) -> anyhow::Result<()> {
        let response = self
            .http
            .post(Self::endpoint(config))
            .header("Open-Shock-Token", config.api_token.trim())
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&Self::payload(command, shocker_ids))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("openshock returned {status}: {}", error_detail(&body));
        }
        Ok(())
    }
}

fn error_detail(body: &str) -> String {
    let trimmed = body.trim_start();
    if trimmed.starts_with('<') {
        return String::from(
            "an HTML error page (the request was blocked before it reached the API)",
        );
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let mut parts = Vec::new();
        if let Some(title) = json["title"].as_str() {
            parts.push(title.to_string());
        }
        if let Some(errors) = json["errors"].as_object() {
            for (field, messages) in errors {
                if let Some(first) = messages
                    .as_array()
                    .and_then(|m| m.first())
                    .and_then(|m| m.as_str())
                {
                    parts.push(format!("{field}: {first}"));
                }
            }
        }
        if !parts.is_empty() {
            return parts.join("; ").chars().take(400).collect();
        }
        if let Some(message) = json["message"].as_str() {
            return message.chars().take(400).collect();
        }
    }
    body.chars().take(200).collect()
}

impl Default for OpenShockClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadass_shared::{OpenShockConfig, Shocker};

    fn roster_config() -> OpenShockConfig {
        OpenShockConfig {
            shockers: vec![Shocker {
                id: String::from("abc123"),
                name: String::from("Left hand"),
            }],
            ..OpenShockConfig::default()
        }
    }

    #[test]
    fn payload_matches_openshock_control_schema() {
        let command = ControlCommand {
            kind: ShockKind::Shock,
            intensity: 250,
            duration_ms: 90_000,
        };
        let payload =
            OpenShockClient::payload(command, &[String::from("abc123"), String::from("def456")]);
        let shocks = payload["shocks"].as_array().expect("shocks array");
        assert_eq!(shocks.len(), 2);
        assert_eq!(shocks[0]["id"], "abc123");
        assert_eq!(shocks[1]["id"], "def456");
        assert_eq!(shocks[0]["type"], "shock");
        assert_eq!(shocks[0]["intensity"], 100);
        assert_eq!(shocks[0]["duration"], 30_000);
        assert_eq!(payload["custom"], false);
    }

    #[test]
    fn configured_requires_token_and_roster() {
        let mut config = OpenShockConfig::default();
        assert!(!OpenShockClient::configured(&config));
        config.api_token = String::from("tok");
        assert!(!OpenShockClient::configured(&config), "no shocker yet");
        config.shockers = vec![Shocker {
            id: String::from("  "),
            name: String::from("blank"),
        }];
        assert!(
            !OpenShockClient::configured(&config),
            "blank shocker id does not count"
        );
        config = roster_config();
        config.enabled = true;
        config.api_token = String::from("tok");
        assert!(OpenShockClient::configured(&config));
    }

    #[test]
    fn html_error_bodies_become_a_short_hint() {
        let blocked = "<!DOCTYPE html>\n<!--[if lt IE 7]> <html> <![endif]-->";
        assert_eq!(
            error_detail(blocked),
            "an HTML error page (the request was blocked before it reached the API)",
        );
        let long: String = "x".repeat(500);
        assert_eq!(error_detail(&long).chars().count(), 200);
    }

    #[test]
    fn validation_errors_are_summarized_field_by_field() {
        let body = r#"{"type":"Validation.Error","title":"One or more validation errors occurred","status":400,"errors":{"body":["The body field is required."],"$.shocks[0].id":["The JSON value could not be converted to System.Guid."]}}"#;
        assert_eq!(
            error_detail(body),
            "One or more validation errors occurred; \
             $.shocks[0].id: The JSON value could not be converted to System.Guid.; \
             body: The body field is required.",
        );
    }
}
