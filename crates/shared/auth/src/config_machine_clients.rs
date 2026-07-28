//! Machine (client-credentials / private_key_jwt) client and enterprise
//! JWT-bearer issuer configuration. Split out of `config.rs` to stay under
//! the PATTERNS.md module size hard limit.
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineClientConfig {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub jwks: Option<serde_json::Value>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
}

impl std::fmt::Debug for MachineClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MachineClientConfig")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("jwks", &self.jwks.as_ref().map(|_| "<configured>"))
            .field("scopes", &self.scopes)
            .field("resources", &self.resources)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnterpriseIssuerConfig {
    pub issuer: String,
    #[serde(default)]
    pub jwks_uri: Option<Url>,
    #[serde(default)]
    pub jwks: Option<serde_json::Value>,
    #[serde(default)]
    pub allowed_client_ids: Vec<String>,
}
