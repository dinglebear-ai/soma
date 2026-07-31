//! Capability broker for persistent Python workers.

use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{Value, json};
use soma_provider_core::{BrokerCapability, HostCapabilities, NetworkCapability};
use url::Url;

use crate::{
    broker_state::BrokerStateStore,
    python_protocol::{
        PythonActorContext, PythonRunnerError, PythonRunnerErrorCode, PythonRunnerErrorPhase,
        PythonRunnerHostCall,
    },
};

const MAX_AUDIT_EVENTS: usize = 256;
type HostResult<T> = Result<T, Box<PythonRunnerError>>;

/// Ambient-authority posture for Python execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonExecutionProfile {
    Disabled,
    #[default]
    Trusted,
    Brokered,
}

/// Bounded, secret-free record of one broker decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonHostAuditEvent {
    pub unix_ms: u64,
    pub invocation_id: String,
    pub operation: &'static str,
    pub allowed: bool,
    pub detail: String,
}

/// Host-side services and authority declarations for one provider worker.
pub struct PythonHostBroker {
    profile: PythonExecutionProfile,
    network: Option<NetworkCapability>,
    broker: Option<BrokerCapability>,
    max_http_response_bytes: usize,
    state: Result<Arc<BrokerStateStore>, String>,
    audit: Mutex<VecDeque<PythonHostAuditEvent>>,
    cancelled: Arc<AtomicBool>,
}

impl std::fmt::Debug for PythonHostBroker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PythonHostBroker")
            .field("profile", &self.profile)
            .field("network", &self.network)
            .field("broker", &self.broker)
            .finish_non_exhaustive()
    }
}

impl PythonHostBroker {
    #[must_use]
    pub fn new(
        profile: PythonExecutionProfile,
        capabilities: &HostCapabilities,
        cancelled: Arc<AtomicBool>,
    ) -> Arc<Self> {
        Arc::new(Self {
            profile,
            network: capabilities.network.clone(),
            broker: capabilities.broker.clone(),
            max_http_response_bytes: 256 * 1024,
            state: BrokerStateStore::configured(),
            audit: Mutex::new(VecDeque::new()),
            cancelled,
        })
    }

    #[must_use]
    pub fn profile(&self) -> PythonExecutionProfile {
        self.profile
    }

    #[must_use]
    pub fn audit_events(&self) -> Vec<PythonHostAuditEvent> {
        self.audit
            .lock()
            .expect("Python host audit lock should not be poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub(crate) fn begin_invocation(&self) {
        self.cancelled.store(false, Ordering::Release);
    }

    pub(crate) fn cancel_invocation(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub async fn execute(
        &self,
        call: &PythonRunnerHostCall,
        actor: Option<&PythonActorContext>,
    ) -> HostResult<Value> {
        if self.profile == PythonExecutionProfile::Disabled {
            return Err(self.denied(call, "Python execution is disabled"));
        }
        let result = match call {
            PythonRunnerHostCall::Http {
                invocation_id,
                request,
                ..
            } => {
                self.require_scope(actor, false, call)?;
                self.http(invocation_id, request).await
            }
            PythonRunnerHostCall::Secret {
                invocation_id,
                name,
                ..
            } => {
                self.require_scope(actor, false, call)?;
                self.secret(invocation_id, name)
            }
            PythonRunnerHostCall::StateGet {
                invocation_id, key, ..
            } => {
                self.require_scope(actor, false, call)?;
                self.state_get(invocation_id, key).await
            }
            PythonRunnerHostCall::StatePut {
                invocation_id,
                key,
                value,
                ..
            } => {
                self.require_scope(actor, true, call)?;
                self.state_put(invocation_id, key, value).await
            }
            PythonRunnerHostCall::Log {
                invocation_id,
                level,
                message,
                fields,
                ..
            } => {
                self.require_scope(actor, false, call)?;
                self.log(invocation_id, level, message, fields)
            }
            PythonRunnerHostCall::Metric {
                invocation_id,
                name,
                value,
                attributes,
                ..
            } => {
                self.require_scope(actor, false, call)?;
                self.metric(invocation_id, name, value, attributes)
            }
            PythonRunnerHostCall::Progress {
                invocation_id,
                current,
                total,
                message,
                ..
            } => {
                self.require_scope(actor, false, call)?;
                self.progress(invocation_id, *current, *total, message.as_deref())
            }
            PythonRunnerHostCall::Cancelled { invocation_id, .. } => {
                let value = self.cancelled.load(Ordering::Acquire);
                self.record(invocation_id, "cancelled", true, "queried".to_owned());
                Ok(Value::Bool(value))
            }
        };
        if let Err(error) = &result {
            self.record(
                invocation_id(call),
                operation(call),
                false,
                error.public_message.clone(),
            );
        }
        result
    }

    fn require_scope(
        &self,
        actor: Option<&PythonActorContext>,
        write: bool,
        call: &PythonRunnerHostCall,
    ) -> HostResult<()> {
        let actor =
            actor.ok_or_else(|| self.denied(call, "authenticated actor context is required"))?;
        let allowed = actor
            .scopes
            .iter()
            .any(|scope| scope == "soma:write" || (!write && scope == "soma:read"));
        if allowed {
            Ok(())
        } else {
            Err(self.denied(call, "actor scopes do not authorize this host operation"))
        }
    }

    async fn http(&self, invocation_id: &str, request: &Value) -> HostResult<Value> {
        let capability = self
            .network
            .as_ref()
            .filter(|capability| capability.enabled)
            .ok_or_else(|| self.policy_error("provider did not declare network capability"))?;
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET");
        let raw_url = request
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| self.policy_error("HTTP request URL is required"))?;
        let url =
            Url::parse(raw_url).map_err(|_| self.policy_error("HTTP request URL is invalid"))?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err(self.policy_error("brokered HTTP requires HTTPS without URL credentials"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| self.policy_error("HTTP request host is required"))?
            .to_owned();
        if !capability
            .allowed_hosts
            .iter()
            .any(|allowed| allowed == &host)
        {
            return Err(self.policy_error("HTTP request host is not declared"));
        }
        let port = url.port_or_known_default().unwrap_or(443);
        let addresses = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|_| self.policy_error("HTTP host resolution failed"))?
            .collect::<Vec<_>>();
        if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
            return Err(self.policy_error("HTTP host resolved to a non-public address"));
        }

        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .https_only(true);
        for address in addresses {
            builder = builder.resolve(&host, address);
        }
        let client = builder
            .build()
            .map_err(|_| self.policy_error("HTTP client initialization failed"))?;
        let method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| self.policy_error("HTTP method is invalid"))?;
        let mut outbound = client.request(method, url);
        if let Some(headers) = request.get("headers").and_then(Value::as_object) {
            for (name, value) in headers {
                if forbidden_forwarded_header(name) {
                    return Err(self.policy_error("HTTP header is controlled by the broker"));
                }
                let value = value
                    .as_str()
                    .ok_or_else(|| self.policy_error("HTTP header values must be strings"))?;
                outbound = outbound.header(name, value);
            }
        }
        if let Some(body) = request.get("body_base64").and_then(Value::as_str) {
            use base64::Engine as _;

            let body = base64::engine::general_purpose::STANDARD
                .decode(body)
                .map_err(|_| self.policy_error("HTTP request body_base64 is invalid"))?;
            outbound = outbound.body(body);
        } else if let Some(body) = request.get("body").and_then(Value::as_str) {
            // Protocol 1.0 compatibility for older SDKs.
            outbound = outbound.body(body.to_owned());
        }
        let mut response = outbound
            .send()
            .await
            .map_err(|_| self.policy_error("HTTP request failed"))?;
        if response.status().is_redirection() {
            return Err(
                self.policy_error("HTTP redirects are not followed by the capability broker")
            );
        }
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > self.max_http_response_bytes as u64)
        {
            return Err(self.policy_error("HTTP response exceeds broker limit"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| self.policy_error("HTTP response body failed"))?
        {
            if bytes.len().saturating_add(chunk.len()) > self.max_http_response_bytes {
                return Err(self.policy_error("HTTP response exceeds broker limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        self.record(
            invocation_id,
            "http",
            true,
            format!(
                "https://{host}:{port} status={status} bytes={}",
                bytes.len()
            ),
        );
        use base64::Engine as _;

        let mut result = json!({
            "status": status,
            "body_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
        if let Ok(body) = String::from_utf8(bytes)
            && let Some(object) = result.as_object_mut()
        {
            object.insert("body".to_owned(), Value::String(body));
        }
        Ok(result)
    }

    fn secret(&self, invocation_id: &str, name: &str) -> HostResult<Value> {
        let capability = self.broker_capability()?;
        if !capability
            .secret_names
            .iter()
            .any(|allowed| allowed == name)
        {
            return Err(self.policy_error("secret name is not declared"));
        }
        let variable = crate::secret_name::environment_name(name)
            .map_err(|message| self.policy_error(&message))?;
        let secret = std::env::var(variable)
            .map_err(|_| self.policy_error("declared secret is unavailable"))?;
        self.record(invocation_id, "secret", true, name.to_owned());
        Ok(Value::String(secret))
    }

    async fn state_get(&self, invocation_id: &str, key: &str) -> HostResult<Value> {
        let namespace = self.state_namespace()?.to_owned();
        let key = key.to_owned();
        let task_key = key.clone();
        let state = self.state_store()?;
        let cancelled = self.cancelled.clone();
        let deadline = Instant::now() + Duration::from_secs(10);
        let result = tokio::task::spawn_blocking(move || {
            state.get(&namespace, &task_key, deadline, Some(&cancelled))
        })
        .await
        .map_err(|_| self.policy_error("provider state task failed"))?
        .map_err(|message| self.policy_error(&message))?;
        self.record(invocation_id, "state.get", true, key);
        Ok(result)
    }

    async fn state_put(&self, invocation_id: &str, key: &str, value: &Value) -> HostResult<Value> {
        let capability = self.broker_capability()?;
        if !capability.state_write {
            return Err(self.policy_error("provider did not declare state write access"));
        }
        let namespace = self.state_namespace()?.to_owned();
        let state = self.state_store()?;
        let key_owned = key.to_owned();
        let value = value.clone();
        let cancelled = self.cancelled.clone();
        let deadline = Instant::now() + Duration::from_secs(10);
        tokio::task::spawn_blocking(move || {
            state.put(&namespace, &key_owned, &value, deadline, Some(&cancelled))
        })
        .await
        .map_err(|_| self.policy_error("provider state task failed"))?
        .map_err(|message| self.policy_error(&message))?;
        self.record(invocation_id, "state.put", true, key.to_owned());
        Ok(Value::Null)
    }

    fn log(
        &self,
        invocation_id: &str,
        level: &str,
        message: &str,
        fields: &Value,
    ) -> HostResult<Value> {
        if !self.broker_capability()?.logging {
            return Err(self.policy_error("provider did not declare structured logging"));
        }
        let message = self.public_diagnostic(message);
        tracing::info!(
            provider_invocation = invocation_id,
            provider_level = level,
            message,
            fields = %self.public_diagnostic(&fields.to_string()),
            "Python provider structured log"
        );
        self.record(invocation_id, "log", true, level.to_owned());
        Ok(Value::Null)
    }

    fn metric(
        &self,
        invocation_id: &str,
        name: &str,
        value: &serde_json::Number,
        attributes: &Value,
    ) -> HostResult<Value> {
        if !self.broker_capability()?.metrics {
            return Err(self.policy_error("provider did not declare metrics"));
        }
        tracing::info!(
            provider_invocation = invocation_id,
            metric = name,
            value = %value,
            attributes = %self.public_diagnostic(&attributes.to_string()),
            "Python provider metric"
        );
        self.record(invocation_id, "metric", true, name.to_owned());
        Ok(Value::Null)
    }

    fn progress(
        &self,
        invocation_id: &str,
        current: u64,
        total: Option<u64>,
        message: Option<&str>,
    ) -> HostResult<Value> {
        if !self.broker_capability()?.progress {
            return Err(self.policy_error("provider did not declare progress"));
        }
        tracing::info!(
            provider_invocation = invocation_id,
            current,
            ?total,
            message = %self.public_diagnostic(message.unwrap_or_default()),
            "Python provider progress"
        );
        self.record(
            invocation_id,
            "progress",
            true,
            format!("{current}/{total:?}"),
        );
        Ok(Value::Null)
    }

    fn broker_capability(&self) -> HostResult<&BrokerCapability> {
        self.broker
            .as_ref()
            .filter(|capability| capability.enabled)
            .ok_or_else(|| self.policy_error("provider did not declare broker capabilities"))
    }

    fn state_namespace(&self) -> HostResult<&str> {
        self.broker_capability()?
            .state_namespace
            .as_deref()
            .ok_or_else(|| self.policy_error("provider did not declare a state namespace"))
    }

    fn state_store(&self) -> HostResult<Arc<BrokerStateStore>> {
        self.state
            .as_ref()
            .map(Arc::clone)
            .map_err(|message| self.policy_error(message))
    }

    fn denied(&self, call: &PythonRunnerHostCall, message: &str) -> Box<PythonRunnerError> {
        self.record(
            invocation_id(call),
            operation(call),
            false,
            message.to_owned(),
        );
        self.policy_error(message)
    }

    fn policy_error(&self, message: &str) -> Box<PythonRunnerError> {
        Box::new(PythonRunnerError {
            code: PythonRunnerErrorCode::PythonPolicyDenied,
            phase: PythonRunnerErrorPhase::Policy,
            provider: None,
            source: None,
            generation_id: None,
            action: None,
            retryable: false,
            public_message: message.to_owned(),
        })
    }

    fn record(&self, invocation_id: &str, operation: &'static str, allowed: bool, detail: String) {
        let mut audit = self
            .audit
            .lock()
            .expect("Python host audit lock should not be poisoned");
        if audit.len() == MAX_AUDIT_EVENTS {
            audit.pop_front();
        }
        audit.push_back(PythonHostAuditEvent {
            unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            invocation_id: invocation_id.to_owned(),
            operation,
            allowed,
            detail: self.public_diagnostic(&detail),
        });
    }

    fn public_diagnostic(&self, message: &str) -> String {
        let names = self
            .broker
            .as_ref()
            .map(|broker| broker.secret_names.as_slice())
            .unwrap_or_default();
        crate::secret_name::redact(message, names)
    }
}

fn invocation_id(call: &PythonRunnerHostCall) -> &str {
    match call {
        PythonRunnerHostCall::Http { invocation_id, .. }
        | PythonRunnerHostCall::Secret { invocation_id, .. }
        | PythonRunnerHostCall::StateGet { invocation_id, .. }
        | PythonRunnerHostCall::StatePut { invocation_id, .. }
        | PythonRunnerHostCall::Log { invocation_id, .. }
        | PythonRunnerHostCall::Metric { invocation_id, .. }
        | PythonRunnerHostCall::Progress { invocation_id, .. }
        | PythonRunnerHostCall::Cancelled { invocation_id, .. } => invocation_id,
    }
}

fn operation(call: &PythonRunnerHostCall) -> &'static str {
    match call {
        PythonRunnerHostCall::Http { .. } => "http",
        PythonRunnerHostCall::Secret { .. } => "secret",
        PythonRunnerHostCall::StateGet { .. } => "state.get",
        PythonRunnerHostCall::StatePut { .. } => "state.put",
        PythonRunnerHostCall::Log { .. } => "log",
        PythonRunnerHostCall::Metric { .. } => "metric",
        PythonRunnerHostCall::Progress { .. } => "progress",
        PythonRunnerHostCall::Cancelled { .. } => "cancelled",
    }
}

fn forbidden_forwarded_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "content-length"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, third, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                || first == 0
                || (first == 100 && (64..=127).contains(&second))
                || (first == 192 && second == 0 && third == 0)
                || (first == 192 && second == 88 && third == 99)
                || (first == 198 && (18..=19).contains(&second))
                || first >= 240)
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            (0x2000..=0x3fff).contains(&segments[0])
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
                && !ip.is_multicast()
        }
    }
}

#[allow(dead_code)]
fn _socket_address_is_public(address: SocketAddr) -> bool {
    public_ip(address.ip())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::python_protocol::{PythonActorContext, PythonRunnerHostCall};

    fn capabilities(namespace: &str) -> HostCapabilities {
        HostCapabilities {
            broker: Some(BrokerCapability {
                enabled: true,
                state_namespace: Some(namespace.to_owned()),
                state_write: true,
                logging: true,
                metrics: true,
                progress: true,
                ..BrokerCapability::default()
            }),
            ..HostCapabilities::default()
        }
    }

    fn actor(scopes: &[&str]) -> PythonActorContext {
        PythonActorContext {
            actor_id: "actor".to_owned(),
            scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        }
    }

    #[tokio::test]
    async fn state_is_namespaced_and_actor_write_scope_is_required() {
        let mut broker = PythonHostBroker::new(
            PythonExecutionProfile::Brokered,
            &capabilities("provider-a"),
            Arc::new(AtomicBool::new(false)),
        );
        Arc::get_mut(&mut broker).unwrap().state = Ok(BrokerStateStore::in_memory_for_test());
        let denied = broker
            .execute(
                &PythonRunnerHostCall::StatePut {
                    request_id: 1,
                    invocation_id: "invocation".to_owned(),
                    key: "count".to_owned(),
                    value: json!(1),
                },
                Some(&actor(&["soma:read"])),
            )
            .await
            .expect_err("read scope must not grant state writes");
        assert_eq!(denied.code, PythonRunnerErrorCode::PythonPolicyDenied);

        broker
            .execute(
                &PythonRunnerHostCall::StatePut {
                    request_id: 2,
                    invocation_id: "invocation".to_owned(),
                    key: "count".to_owned(),
                    value: json!(2),
                },
                Some(&actor(&["soma:write"])),
            )
            .await
            .expect("write scope and provider declaration intersect");
        let value = broker
            .execute(
                &PythonRunnerHostCall::StateGet {
                    request_id: 3,
                    invocation_id: "invocation".to_owned(),
                    key: "count".to_owned(),
                },
                Some(&actor(&["soma:read"])),
            )
            .await
            .expect("read scope can access declared state");
        assert_eq!(value, json!(2));
    }

    #[tokio::test]
    async fn disabled_profile_and_undeclared_services_fail_closed() {
        let disabled = PythonHostBroker::new(
            PythonExecutionProfile::Disabled,
            &HostCapabilities::default(),
            Arc::new(AtomicBool::new(false)),
        );
        let error = disabled
            .execute(
                &PythonRunnerHostCall::Cancelled {
                    request_id: 1,
                    invocation_id: "invocation".to_owned(),
                },
                None,
            )
            .await
            .expect_err("disabled profile rejects host calls");
        assert_eq!(error.code, PythonRunnerErrorCode::PythonPolicyDenied);
    }

    #[tokio::test]
    async fn missing_actor_context_fails_closed() {
        let broker = PythonHostBroker::new(
            PythonExecutionProfile::Brokered,
            &capabilities("provider-a"),
            Arc::new(AtomicBool::new(false)),
        );
        let error = broker
            .execute(
                &PythonRunnerHostCall::StateGet {
                    request_id: 1,
                    invocation_id: "invocation".to_owned(),
                    key: "count".to_owned(),
                },
                None,
            )
            .await
            .expect_err("brokered host services require an authenticated actor");
        assert_eq!(error.code, PythonRunnerErrorCode::PythonPolicyDenied);
    }

    #[test]
    fn diagnostics_and_network_targets_are_conservative() {
        let broker = PythonHostBroker::new(
            PythonExecutionProfile::Trusted,
            &HostCapabilities::default(),
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(
            broker.public_diagnostic("Authorization: bearer value"),
            "[redacted]"
        );
        assert!(!public_ip("127.0.0.1".parse().unwrap()));
        assert!(!public_ip("10.0.0.1".parse().unwrap()));
        assert!(!public_ip("100.64.0.1".parse().unwrap()));
        assert!(!public_ip("224.0.0.1".parse().unwrap()));
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        assert!(forbidden_forwarded_header("Host"));
        assert!(forbidden_forwarded_header("transfer-encoding"));
        assert!(!forbidden_forwarded_header("authorization"));
    }
}
