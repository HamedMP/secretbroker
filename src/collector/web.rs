use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, sync::oneshot, time::timeout};

use crate::{
    collector::{CollectedSecret, VariableRequest},
    secret::SecretValue,
};

#[derive(Clone)]
struct WebState {
    authority: String,
    origin: String,
    capability: Arc<SecretValue>,
    variables: Arc<Vec<VariableRequest>>,
    result_tx: Arc<Mutex<Option<oneshot::Sender<Vec<CollectedSecret>>>>>,
    shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    document: Arc<str>,
    csp: Arc<str>,
}

#[derive(Deserialize)]
struct Submission {
    values: HashMap<String, String>,
}

pub async fn collect(
    variables: &[VariableRequest],
    allow_empty: bool,
    lifetime: Duration,
) -> Result<Vec<CollectedSecret>> {
    if variables.is_empty() {
        bail!("browser collection requires at least one variable");
    }

    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .context("cannot bind secure browser form to loopback")?;
    let address = listener.local_addr()?;
    let authority = address.to_string();
    let origin = format!("http://{authority}");
    let capability = Arc::new(SecretValue::new(random_token()?));
    let nonce = random_token()?;
    let document = build_document(variables, &nonce)?;
    let csp = format!(
        "default-src 'none'; script-src 'nonce-{nonce}'; style-src 'nonce-{nonce}'; connect-src 'self'; form-action 'none'; frame-ancestors 'none'; base-uri 'none'"
    );

    let (result_tx, result_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let state = WebState {
        authority,
        origin: origin.clone(),
        capability: Arc::clone(&capability),
        variables: Arc::new(variables.to_vec()),
        result_tx: Arc::new(Mutex::new(Some(result_tx))),
        shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
        document: Arc::from(document),
        csp: Arc::from(csp),
    };

    let app = Router::new()
        .route("/", get(index))
        .route(
            "/submit",
            post(move |state, headers, body| submit(state, headers, body, allow_empty)),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .fallback(not_found)
        .with_state(state.clone());

    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });

    let url = format!("{origin}/#{}", capability.expose());
    if let Err(error) = open::that(&url) {
        signal_shutdown(&state);
        let _ = server.await;
        return Err(error).context("cannot open the secure browser form; use --terminal instead");
    }

    let result = match timeout(lifetime, result_rx).await {
        Ok(Ok(values)) => Ok(values),
        Ok(Err(_)) => Err(anyhow!("secure browser form closed before submission")),
        Err(_) => Err(anyhow!("secure browser form expired")),
    };
    signal_shutdown(&state);
    match server.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(error).context("secure browser server failed"),
        Err(error) => return Err(error).context("secure browser server task failed"),
    }
    result
}

async fn index(State(state): State<WebState>, headers: HeaderMap) -> Response {
    if !valid_host(&state, &headers) {
        return secure_response(
            &state,
            (StatusCode::BAD_REQUEST, "Invalid host").into_response(),
        );
    }
    secure_response(&state, Html(state.document.to_string()).into_response())
}

async fn submit(
    State(state): State<WebState>,
    headers: HeaderMap,
    Json(submission): Json<Submission>,
    allow_empty: bool,
) -> Response {
    if !valid_host(&state, &headers) || !valid_origin(&state, &headers) {
        return secure_response(
            &state,
            (StatusCode::FORBIDDEN, "Request rejected").into_response(),
        );
    }
    if !valid_capability(&state, &headers) {
        return secure_response(
            &state,
            (StatusCode::UNAUTHORIZED, "Request rejected").into_response(),
        );
    }

    let mut values = submission.values;
    if values.len() != state.variables.len() {
        return secure_response(
            &state,
            (StatusCode::BAD_REQUEST, "All requested fields are required").into_response(),
        );
    }

    let mut collected = Vec::with_capacity(state.variables.len());
    for variable in state.variables.iter() {
        let Some(value) = values.remove(variable.name.as_str()) else {
            return secure_response(
                &state,
                (StatusCode::BAD_REQUEST, "All requested fields are required").into_response(),
            );
        };
        if value.is_empty() && !allow_empty {
            return secure_response(
                &state,
                (StatusCode::BAD_REQUEST, "Empty values are not allowed").into_response(),
            );
        }
        collected.push((variable.name.clone(), SecretValue::new(value)));
    }
    if !values.is_empty() {
        return secure_response(
            &state,
            (StatusCode::BAD_REQUEST, "Unexpected fields").into_response(),
        );
    }

    let sender = state
        .result_tx
        .lock()
        .expect("result sender poisoned")
        .take();
    let Some(sender) = sender else {
        return secure_response(
            &state,
            (StatusCode::CONFLICT, "Request already completed").into_response(),
        );
    };
    if sender.send(collected).is_err() {
        return secure_response(
            &state,
            (StatusCode::GONE, "Request expired").into_response(),
        );
    }
    signal_shutdown(&state);
    secure_response(
        &state,
        (
            StatusCode::OK,
            "Values received. Return to the terminal to confirm secure storage.",
        )
            .into_response(),
    )
}

async fn not_found(State(state): State<WebState>) -> Response {
    secure_response(&state, (StatusCode::NOT_FOUND, "Not found").into_response())
}

fn valid_host(state: &WebState, headers: &HeaderMap) -> bool {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.authority)
}

fn valid_origin(state: &WebState, headers: &HeaderMap) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.origin)
}

fn valid_capability(state: &WebState, headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    value
        .as_bytes()
        .ct_eq(state.capability.expose().as_bytes())
        .into()
}

fn secure_response(state: &WebState, mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
    );
    if let Ok(value) = HeaderValue::from_str(&state.csp) {
        headers.insert(header::CONTENT_SECURITY_POLICY, value);
    }
    response
}

fn signal_shutdown(state: &WebState) {
    if let Some(sender) = state
        .shutdown_tx
        .lock()
        .expect("shutdown sender poisoned")
        .take()
    {
        let _ = sender.send(());
    }
}

fn random_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("cannot obtain secure randomness")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn build_document(variables: &[VariableRequest], nonce: &str) -> Result<String> {
    let mut variables_json = serde_json::to_string(variables)?;
    variables_json = variables_json
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>SecretBroker</title>
<style nonce="{nonce}">
:root {{ color-scheme: light dark; font-family: ui-sans-serif, system-ui, sans-serif; }}
body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: #111827; color: #f9fafb; }}
main {{ width: min(36rem, calc(100vw - 2rem)); background: #1f2937; border: 1px solid #374151; border-radius: 1rem; padding: 1.5rem; box-shadow: 0 1rem 3rem #0008; }}
h1 {{ margin-top: 0; }} p {{ color: #d1d5db; }} label {{ display: block; margin: 1rem 0; font-weight: 650; }}
small {{ display: block; color: #9ca3af; margin: .25rem 0 .5rem; font-weight: 400; }}
input {{ box-sizing: border-box; width: 100%; padding: .75rem; border-radius: .5rem; border: 1px solid #4b5563; background: #111827; color: #f9fafb; }}
button {{ width: 100%; padding: .8rem; border: 0; border-radius: .5rem; background: #2563eb; color: white; font-weight: 700; cursor: pointer; }}
button:disabled {{ opacity: .6; cursor: wait; }} #status {{ min-height: 1.5rem; }}
</style>
</head>
<body><main>
<h1>SecretBroker</h1>
<p>Enter the requested values. They are sent only to the local SecretBroker process and are never saved by this page.</p>
<form id="form"><div id="fields"></div><button type="submit">Store securely</button></form>
<p id="status" role="status" aria-live="polite"></p>
</main>
<script nonce="{nonce}">
'use strict';
const capability = location.hash.slice(1);
history.replaceState(null, '', '/');
const variables = {variables_json};
const fields = document.getElementById('fields');
for (const variable of variables) {{
  const label = document.createElement('label');
  label.append(document.createTextNode(variable.name));
  if (variable.description) {{ const small = document.createElement('small'); small.textContent = variable.description; label.append(small); }}
  const input = document.createElement('input'); input.type = 'password'; input.name = variable.name; input.autocomplete = 'off'; input.spellcheck = false; input.required = true;
  label.append(input); fields.append(label);
}}
document.getElementById('form').addEventListener('submit', async event => {{
  event.preventDefault(); const button = event.currentTarget.querySelector('button'); button.disabled = true;
  const values = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {{
    const response = await fetch('/submit', {{ method: 'POST', headers: {{ 'Authorization': `Bearer ${{capability}}`, 'Content-Type': 'application/json' }}, body: JSON.stringify({{values}}), credentials: 'omit', cache: 'no-store' }});
    const message = await response.text(); document.getElementById('status').textContent = message;
    if (response.ok) {{ for (const input of event.currentTarget.querySelectorAll('input')) input.value = ''; event.currentTarget.remove(); }} else button.disabled = false;
  }} catch (_) {{ document.getElementById('status').textContent = 'The local request closed or failed.'; button.disabled = false; }}
}});
</script></body></html>"#
    ))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use axum::{
        Json,
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode, header},
    };
    use tokio::sync::oneshot;

    use super::{Submission, WebState, build_document, index, submit};
    use crate::{collector::VariableRequest, env_name::EnvName, secret::SecretValue};

    #[test]
    fn escapes_descriptions_before_embedding_json() {
        let document = build_document(
            &[VariableRequest {
                name: EnvName::new("TOKEN").expect("name"),
                description: Some("</script><script>bad()</script>".to_owned()),
            }],
            "nonce",
        )
        .expect("document");
        assert!(!document.contains("</script><script>bad()"));
        assert!(document.contains("\\u003c/script\\u003e"));
    }

    #[tokio::test]
    async fn submission_requires_local_origin_and_capability() {
        let (result_tx, result_rx) = oneshot::channel();
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        let state = WebState {
            authority: "127.0.0.1:12345".to_owned(),
            origin: "http://127.0.0.1:12345".to_owned(),
            capability: Arc::new(SecretValue::new("test-capability".to_owned())),
            variables: Arc::new(vec![VariableRequest {
                name: EnvName::new("TOKEN").expect("name"),
                description: None,
            }]),
            result_tx: Arc::new(Mutex::new(Some(result_tx))),
            shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
            document: Arc::from("document"),
            csp: Arc::from("default-src 'none'"),
        };

        let mut invalid_headers = HeaderMap::new();
        invalid_headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:12345"));
        invalid_headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.test"),
        );
        invalid_headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-capability"),
        );
        let invalid = submit(
            State(state.clone()),
            invalid_headers,
            Json(Submission {
                values: HashMap::from([("TOKEN".to_owned(), "synthetic".to_owned())]),
            }),
            false,
        )
        .await;
        assert_eq!(invalid.status(), StatusCode::FORBIDDEN);

        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:12345"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:12345"),
        );
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-capability"),
        );
        let response = submit(
            State(state),
            headers,
            Json(Submission {
                values: HashMap::from([("TOKEN".to_owned(), "synthetic".to_owned())]),
            }),
            false,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "no-store, max-age=0"
        );
        let collected = result_rx.await.expect("collected values");
        assert_eq!(collected[0].1.expose(), "synthetic");
    }

    #[tokio::test]
    async fn index_rejects_dns_rebinding_host() {
        let (result_tx, _result_rx) = oneshot::channel();
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        let state = WebState {
            authority: "127.0.0.1:12345".to_owned(),
            origin: "http://127.0.0.1:12345".to_owned(),
            capability: Arc::new(SecretValue::new("test-capability".to_owned())),
            variables: Arc::new(Vec::new()),
            result_tx: Arc::new(Mutex::new(Some(result_tx))),
            shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
            document: Arc::from("document"),
            csp: Arc::from("default-src 'none'"),
        };
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("attacker.test"));
        let response = index(State(state), headers).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
