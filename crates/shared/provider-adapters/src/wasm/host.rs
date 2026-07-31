use super::*;

pub(super) fn component_linker(engine: &Engine) -> Result<Linker<WasmStoreState>, String> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|error| error.to_string())?;
    let mut host = linker
        .instance("soma:provider/host@1.0.0")
        .map_err(|error| error.to_string())?;
    host.func_wrap("http", |mut store, (request,): (String,)| {
        Ok((component_http(store.data_mut(), &request),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap("secret", |mut store, (name,): (String,)| {
        Ok((component_secret(store.data_mut(), &name),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap("state-get", |mut store, (key,): (String,)| {
        Ok((component_state_get(store.data_mut(), &key),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap("state-put", |mut store, (key, value): (String, String)| {
        Ok((component_state_put(store.data_mut(), &key, &value),))
    })
    .map_err(|error| error.to_string())?;
    host.func_wrap(
        "log",
        |mut store, (level, message, fields): (String, String, String)| {
            Ok((component_log(store.data_mut(), &level, &message, &fields),))
        },
    )
    .map_err(|error| error.to_string())?;
    host.func_wrap(
        "metric",
        |mut store, (name, value, attributes): (String, f64, String)| {
            Ok((component_metric(
                store.data_mut(),
                &name,
                value,
                &attributes,
            ),))
        },
    )
    .map_err(|error| error.to_string())?;
    host.func_wrap(
        "progress",
        |mut store, (current, total, message): (u64, Option<u64>, Option<String>)| {
            Ok((component_progress(
                store.data_mut(),
                current,
                total,
                message.as_deref(),
            ),))
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(linker)
}

fn component_http(state: &WasmStoreState, request: &str) -> Result<String, String> {
    component_require_scope(state, false)?;
    component_remaining(state)?;
    let network = state
        .capabilities
        .network
        .as_ref()
        .filter(|capability| capability.enabled)
        .ok_or_else(|| "network capability not declared".to_owned())?;
    let request: Value =
        serde_json::from_str(request).map_err(|_| "HTTP request JSON is invalid".to_owned())?;
    let raw_url = request
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "HTTP request URL is required".to_owned())?;
    let url = url::Url::parse(raw_url).map_err(|_| "HTTP request URL is invalid".to_owned())?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("component HTTP requires HTTPS without URL credentials".to_owned());
    }
    let hostname = url
        .host_str()
        .ok_or_else(|| "HTTP request host is required".to_owned())?
        .to_ascii_lowercase();
    if !network
        .allowed_hosts
        .iter()
        .any(|allowed| allowed.trim().eq_ignore_ascii_case(&hostname))
    {
        return Err("HTTP request host is not declared".to_owned());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = state
        .resolved_hosts
        .get(&hostname)
        .ok_or_else(|| "HTTP host was not resolved before component execution".to_owned())?
        .iter()
        .copied()
        .map(|address| SocketAddr::new(address, port))
        .collect::<Vec<_>>();
    let remaining = component_remaining(state)?;
    let mut client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(remaining)
        .timeout(remaining)
        .https_only(true);
    for address in addresses {
        client = client.resolve(&hostname, address);
    }
    let client = client.build().map_err(|error| error.to_string())?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .parse::<reqwest::Method>()
        .map_err(|_| "HTTP method is invalid".to_owned())?;
    let mut outbound = client.request(method, url);
    if let Some(headers) = request.get("headers").and_then(Value::as_object) {
        for (name, value) in headers {
            if component_forbidden_header(name) {
                return Err("HTTP header is controlled by the component host".to_owned());
            }
            let value = value
                .as_str()
                .ok_or_else(|| "HTTP header values must be strings".to_owned())?;
            outbound = outbound.header(name, value);
        }
    }
    if let Some(body) = request.get("body_base64").and_then(Value::as_str) {
        use base64::Engine as _;

        let body = base64::engine::general_purpose::STANDARD
            .decode(body)
            .map_err(|_| "HTTP request body_base64 is invalid".to_owned())?;
        outbound = outbound.body(body);
    } else if let Some(body) = request.get("body").and_then(Value::as_str) {
        outbound = outbound.body(body.to_owned());
    }
    let response = outbound
        .send()
        .map_err(|_| "HTTP request failed".to_owned())?;
    component_remaining(state)?;
    if response.status().is_redirection() {
        return Err("HTTP redirects are not followed by the component host".to_owned());
    }
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|length| length > 256 * 1024)
    {
        return Err("HTTP response exceeds component host limit".to_owned());
    }
    let mut body = Vec::new();
    response
        .take(256 * 1024 + 1)
        .read_to_end(&mut body)
        .map_err(|_| "HTTP response body failed".to_owned())?;
    component_remaining(state)?;
    if body.len() > 256 * 1024 {
        return Err("HTTP response exceeds component host limit".to_owned());
    }
    use base64::Engine as _;

    let mut result = serde_json::json!({
        "status": status,
        "body_base64": base64::engine::general_purpose::STANDARD.encode(&body),
    });
    if let Ok(text) = String::from_utf8(body)
        && let Some(object) = result.as_object_mut()
    {
        object.insert("body".to_owned(), Value::String(text));
    }
    serde_json::to_string(&result).map_err(|error| error.to_string())
}

fn component_secret(state: &WasmStoreState, name: &str) -> Result<String, String> {
    component_require_scope(state, false)?;
    let capability = component_broker(state)?;
    if !capability
        .secret_names
        .iter()
        .any(|allowed| allowed == name)
    {
        return Err("secret name is not declared".to_owned());
    }
    let variable = crate::secret_name::environment_name(name)?;
    std::env::var(variable).map_err(|_| "declared secret is unavailable".to_owned())
}

pub(super) fn component_state_get(state: &WasmStoreState, key: &str) -> Result<String, String> {
    component_require_scope(state, false)?;
    let namespace = component_broker(state)?
        .state_namespace
        .as_ref()
        .ok_or_else(|| "state namespace is not declared".to_owned())?;
    let value =
        state
            .state
            .as_deref()
            .map_err(Clone::clone)?
            .get(namespace, key, state.deadline, None)?;
    serde_json::to_string(&value).map_err(|error| error.to_string())
}

pub(super) fn component_state_put(
    state: &WasmStoreState,
    key: &str,
    value: &str,
) -> Result<(), String> {
    component_require_scope(state, true)?;
    let capability = component_broker(state)?;
    if !capability.state_write {
        return Err("state write capability is not declared".to_owned());
    }
    let namespace = capability
        .state_namespace
        .as_ref()
        .ok_or_else(|| "state namespace is not declared".to_owned())?;
    let value: Value =
        serde_json::from_str(value).map_err(|_| "state value JSON is invalid".to_owned())?;
    state
        .state
        .as_deref()
        .map_err(Clone::clone)?
        .put(namespace, key, &value, state.deadline, None)
}

fn component_log(
    state: &WasmStoreState,
    level: &str,
    message: &str,
    fields: &str,
) -> Result<(), String> {
    component_require_scope(state, false)?;
    if !component_broker(state)?.logging {
        return Err("structured logging capability is not declared".to_owned());
    }
    tracing::info!(
        provider_level = level,
        message = %component_diagnostic(state, message),
        fields = %component_diagnostic(state, fields),
        "component provider log"
    );
    Ok(())
}

pub(super) fn component_diagnostic(state: &WasmStoreState, message: &str) -> String {
    let names = state
        .capabilities
        .broker
        .as_ref()
        .map(|broker| broker.secret_names.as_slice())
        .unwrap_or_default();
    crate::secret_name::redact(message, names)
}
