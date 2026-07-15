use std::path::PathBuf;

pub const RESERVED_NAMESPACES: [&str; 3] = ["state", "git", "openapi"];

#[derive(Clone)]
pub enum SpecSource {
    Url(url::Url),
    Path(PathBuf),
}

impl std::fmt::Debug for SpecSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Url(url) => f
                .debug_tuple("Url")
                .field(&crate::ssrf::redact_url(url.as_str()))
                .finish(),
            Self::Path(path) => f.debug_tuple("Path").field(path).finish(),
        }
    }
}

#[derive(Clone)]
pub enum OpenApiCredential {
    ApiKey { header: String, value: String },
    BearerToken(String),
}

impl OpenApiCredential {
    #[must_use]
    pub fn api_key(header: impl Into<String>, value: impl Into<String>) -> Self {
        Self::ApiKey {
            header: header.into(),
            value: value.into(),
        }
    }

    #[must_use]
    pub fn bearer_token(value: impl Into<String>) -> Self {
        Self::BearerToken(value.into())
    }
}

impl std::fmt::Debug for OpenApiCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey { header, .. } => f
                .debug_struct("ApiKey")
                .field("header", header)
                .field("value", &"<redacted>")
                .finish(),
            Self::BearerToken(_) => f.debug_tuple("BearerToken").field(&"<redacted>").finish(),
        }
    }
}

impl serde::Serialize for OpenApiCredential {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        match self {
            Self::ApiKey { header, .. } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "api_key")?;
                map.serialize_entry("header", header)?;
                map.serialize_entry("value", "<redacted>")?;
                map.end()
            }
            Self::BearerToken(_) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "bearer_token")?;
                map.serialize_entry("value", "<redacted>")?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenApiSpecConfig {
    pub label: String,
    pub spec_source: SpecSource,
    pub base_url: url::Url,
    pub allowed_operations: Vec<String>,
    pub credential: Option<OpenApiCredential>,
}

impl OpenApiSpecConfig {
    #[must_use]
    pub fn is_reserved_label(&self) -> bool {
        RESERVED_NAMESPACES.contains(&self.label.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct OpenApiConfig {
    pub specs: Vec<OpenApiSpecConfig>,
}

impl OpenApiConfig {
    #[must_use]
    pub fn empty() -> Self {
        Self { specs: Vec::new() }
    }
}
