use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::{Url, form_urlencoded};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OAuthLoginOptions {
    pub endpoint: String,
    pub browser_open_command: Option<String>,
    pub timeout: Duration,
    pub client_name: String,
}

#[derive(Debug, Clone)]
pub struct OAuthLoginResult {
    pub access_token: String,
    pub account: Option<String>,
    pub issuer: String,
    pub expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ProtectedResourceMetadata {
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    authorization_servers: Vec<String>,
    #[serde(default)]
    scopes_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    registration_endpoint: Option<String>,
    #[serde(default)]
    scopes_supported: Vec<String>,
    #[serde(default)]
    code_challenge_methods_supported: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ClientRegistrationRequest {
    redirect_uris: Vec<String>,
    client_name: String,
    grant_types: Vec<String>,
    response_types: Vec<String>,
    token_endpoint_auth_method: String,
}

#[derive(Debug, Deserialize)]
struct ClientRegistrationResponse {
    client_id: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
}

struct CallbackData {
    code: String,
}

pub fn login(options: OAuthLoginOptions) -> Result<OAuthLoginResult> {
    let endpoint_url = Url::parse(&options.endpoint)
        .with_context(|| format!("invalid MCP endpoint URL: {}", options.endpoint))?;
    let agent = ureq::AgentBuilder::new().timeout(options.timeout).build();

    let protected = discover_protected_resource(&agent, &endpoint_url)?;
    let auth_server_url = protected.authorization_servers.first().ok_or_else(|| {
        anyhow!("protected resource metadata did not include authorization_servers")
    })?;
    let metadata = discover_authorization_server(&agent, auth_server_url)?;

    if !metadata
        .code_challenge_methods_supported
        .iter()
        .any(|method| method == "S256")
    {
        return Err(anyhow!(
            "authorization server does not advertise PKCE S256 support"
        ));
    }

    let registration_endpoint = metadata.registration_endpoint.as_ref().ok_or_else(|| {
        anyhow!("authorization server does not advertise a dynamic registration endpoint")
    })?;

    let listener =
        TcpListener::bind("127.0.0.1:0").context("failed to bind local OAuth callback listener")?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener.local_addr()?.port()
    );

    let client = register_client(
        &agent,
        registration_endpoint,
        &redirect_uri,
        &options.client_name,
    )?;

    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = Uuid::new_v4().to_string();
    let resource = protected
        .resource
        .clone()
        .unwrap_or_else(|| options.endpoint.clone());
    let scopes = select_scopes(&protected, &metadata);
    let authorization_url = authorization_url(
        &metadata.authorization_endpoint,
        &client.client_id,
        &redirect_uri,
        &challenge,
        &state,
        &resource,
        scopes.as_deref(),
    )?;

    eprintln!("Open this URL to authorize {}:", options.client_name);
    eprintln!("{}", authorization_url);
    if let Err(error) = open_browser(&authorization_url, options.browser_open_command.as_deref()) {
        eprintln!("Could not open browser automatically: {error}");
        eprintln!("Open the URL above manually, then complete the login in your browser.");
    }

    let callback = wait_for_callback(listener, &state, options.timeout)?;
    let token = exchange_code(
        &agent,
        &metadata.token_endpoint,
        &client.client_id,
        &redirect_uri,
        &callback.code,
        &verifier,
        &resource,
    )?;

    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err(anyhow!(
            "authorization server returned unsupported token_type {}",
            token.token_type
        ));
    }

    Ok(OAuthLoginResult {
        access_token: token.access_token,
        account: Some(format!("oauth:{}", metadata.issuer)),
        issuer: metadata.issuer,
        expires_in: token.expires_in,
    })
}

fn discover_protected_resource(
    agent: &ureq::Agent,
    endpoint_url: &Url,
) -> Result<ProtectedResourceMetadata> {
    if let Some(metadata_url) = metadata_url_from_challenge(agent, endpoint_url.as_str())? {
        return get_json(agent, &metadata_url).with_context(|| {
            format!("failed to fetch protected resource metadata from {metadata_url}")
        });
    }

    let mut last_error = None;
    for candidate in protected_resource_metadata_candidates(endpoint_url) {
        match get_json(agent, &candidate) {
            Ok(metadata) => return Ok(metadata),
            Err(error) => last_error = Some(error),
        }
    }

    Err(anyhow!(
        "failed to discover protected resource metadata{}",
        last_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default()
    ))
}

fn metadata_url_from_challenge(agent: &ureq::Agent, endpoint: &str) -> Result<Option<String>> {
    match agent.get(endpoint).call() {
        Ok(_) => Ok(None),
        Err(ureq::Error::Status(401, response)) => Ok(response
            .header("www-authenticate")
            .and_then(|header| parse_www_authenticate_param(header, "resource_metadata"))),
        Err(ureq::Error::Status(_, response)) => Ok(response
            .header("www-authenticate")
            .and_then(|header| parse_www_authenticate_param(header, "resource_metadata"))),
        Err(error) => Err(anyhow!(
            "failed to probe MCP endpoint for auth challenge: {error}"
        )),
    }
}

fn discover_authorization_server(
    agent: &ureq::Agent,
    auth_server_url: &str,
) -> Result<AuthorizationServerMetadata> {
    let auth_url = Url::parse(auth_server_url)
        .with_context(|| format!("invalid authorization server URL: {auth_server_url}"))?;
    let mut last_error = None;
    for candidate in authorization_server_metadata_candidates(&auth_url) {
        match get_json(agent, &candidate) {
            Ok(metadata) => return Ok(metadata),
            Err(error) => last_error = Some(error),
        }
    }

    Err(anyhow!(
        "failed to discover authorization server metadata{}",
        last_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default()
    ))
}

fn register_client(
    agent: &ureq::Agent,
    registration_endpoint: &str,
    redirect_uri: &str,
    client_name: &str,
) -> Result<ClientRegistrationResponse> {
    let request = ClientRegistrationRequest {
        redirect_uris: vec![redirect_uri.to_owned()],
        client_name: client_name.to_owned(),
        grant_types: vec!["authorization_code".to_owned()],
        response_types: vec!["code".to_owned()],
        token_endpoint_auth_method: "none".to_owned(),
    };
    agent
        .post(registration_endpoint)
        .send_json(serde_json::to_value(request)?)
        .map_err(ureq_error)
        .and_then(|response| {
            response
                .into_json()
                .context("failed to parse dynamic client registration response")
        })
}

fn authorization_url(
    authorization_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    resource: &str,
    scope: Option<&str>,
) -> Result<String> {
    let mut url = Url::parse(authorization_endpoint)
        .with_context(|| format!("invalid authorization endpoint: {authorization_endpoint}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("response_type", "code");
        pairs.append_pair("client_id", client_id);
        pairs.append_pair("redirect_uri", redirect_uri);
        pairs.append_pair("code_challenge", code_challenge);
        pairs.append_pair("code_challenge_method", "S256");
        pairs.append_pair("state", state);
        pairs.append_pair("resource", resource);
        if let Some(scope) = scope.filter(|scope| !scope.trim().is_empty()) {
            pairs.append_pair("scope", scope);
        }
    }
    Ok(url.to_string())
}

fn exchange_code(
    agent: &ureq::Agent,
    token_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
    resource: &str,
) -> Result<TokenResponse> {
    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("client_id", client_id)
        .append_pair("code_verifier", code_verifier)
        .append_pair("resource", resource)
        .finish();

    agent
        .post(token_endpoint)
        .set("content-type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(ureq_error)
        .and_then(|response| {
            response
                .into_json()
                .context("failed to parse token response")
        })
}

fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<CallbackData> {
    listener
        .set_nonblocking(true)
        .context("failed to set OAuth callback listener nonblocking")?;
    let deadline = Instant::now() + timeout;

    loop {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let request_path = read_request_path(&mut stream)?;
                let callback_url = Url::parse(&format!("http://127.0.0.1{request_path}"))
                    .context("failed to parse OAuth callback request")?;
                let params: std::collections::HashMap<_, _> =
                    callback_url.query_pairs().into_owned().collect();

                if let Some(error) = params.get("error") {
                    let description = params
                        .get("error_description")
                        .map(String::as_str)
                        .unwrap_or("");
                    write_callback_response(&mut stream, false)?;
                    return Err(anyhow!(
                        "authorization server returned error: {error} {description}"
                    ));
                }

                let state = params
                    .get("state")
                    .ok_or_else(|| anyhow!("OAuth callback missing state"))?;
                if state != expected_state {
                    write_callback_response(&mut stream, false)?;
                    return Err(anyhow!("OAuth callback state did not match"));
                }

                let code = params
                    .get("code")
                    .filter(|code| !code.is_empty())
                    .ok_or_else(|| anyhow!("OAuth callback missing code"))?
                    .to_owned();
                write_callback_response(&mut stream, true)?;
                return Ok(CallbackData { code });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(anyhow!("timed out waiting for OAuth browser callback"));
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(anyhow!("failed to accept OAuth callback: {error}")),
        }
    }
}

fn read_request_path(stream: &mut std::net::TcpStream) -> Result<String> {
    let mut buf = [0_u8; 8192];
    let n = stream
        .read(&mut buf)
        .context("failed to read OAuth callback request")?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow!("OAuth callback request was empty"))?;
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != "GET" || !path.starts_with('/') {
        return Err(anyhow!(
            "unexpected OAuth callback request line: {first_line}"
        ));
    }
    Ok(path.to_owned())
}

fn write_callback_response(stream: &mut std::net::TcpStream, ok: bool) -> Result<()> {
    let (status, body) = if ok {
        (
            "200 OK",
            "OAuth login completed. You can close this browser tab and return to the terminal.",
        )
    } else {
        (
            "400 Bad Request",
            "OAuth login failed. Return to the terminal for details.",
        )
    };
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .context("failed to write OAuth callback response")
}

fn get_json<T: for<'de> Deserialize<'de>>(agent: &ureq::Agent, url: &str) -> Result<T> {
    agent
        .get(url)
        .call()
        .map_err(ureq_error)
        .and_then(|response| {
            response
                .into_json()
                .with_context(|| format!("failed to parse JSON from {url}"))
        })
}

fn ureq_error(error: ureq::Error) -> anyhow::Error {
    match error {
        ureq::Error::Status(status, response) => {
            let message = response
                .into_string()
                .unwrap_or_else(|_| "<failed to read response body>".to_owned());
            anyhow!("HTTP {status}: {message}")
        }
        other => anyhow!("{other}"),
    }
}

fn select_scopes(
    protected: &ProtectedResourceMetadata,
    metadata: &AuthorizationServerMetadata,
) -> Option<String> {
    if !protected.scopes_supported.is_empty() {
        return Some(protected.scopes_supported.join(" "));
    }
    if !metadata.scopes_supported.is_empty() {
        return Some(metadata.scopes_supported.join(" "));
    }
    None
}

fn protected_resource_metadata_candidates(endpoint_url: &Url) -> Vec<String> {
    let origin = url_origin(endpoint_url);
    let mut candidates = Vec::new();
    let path = endpoint_url
        .path()
        .trim_start_matches('/')
        .trim_end_matches('/');
    if !path.is_empty() {
        candidates.push(format!(
            "{origin}/.well-known/oauth-protected-resource/{path}"
        ));
    }
    candidates.push(format!("{origin}/.well-known/oauth-protected-resource"));
    candidates
}

fn authorization_server_metadata_candidates(auth_url: &Url) -> Vec<String> {
    let origin = url_origin(auth_url);
    let path = auth_url
        .path()
        .trim_start_matches('/')
        .trim_end_matches('/');
    let mut candidates = Vec::new();
    if !path.is_empty() {
        candidates.push(format!(
            "{origin}/.well-known/oauth-authorization-server/{path}"
        ));
        candidates.push(format!("{origin}/.well-known/openid-configuration/{path}"));
        candidates.push(format!(
            "{}/.well-known/openid-configuration",
            auth_url.as_str().trim_end_matches('/')
        ));
    } else {
        candidates.push(format!("{origin}/.well-known/oauth-authorization-server"));
        candidates.push(format!("{origin}/.well-known/openid-configuration"));
    }
    candidates
}

fn url_origin(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{}://{}:{}", url.scheme(), host, port),
        None => format!("{}://{}", url.scheme(), host),
    }
}

fn parse_www_authenticate_param(header: &str, key: &str) -> Option<String> {
    for part in header.split(',') {
        let trimmed = part.trim();
        let (_, value) = trimmed.split_once('=')?;
        let name = trimmed.split_once('=')?.0.trim();
        let name = name.strip_prefix("Bearer ").unwrap_or(name).trim();
        if name.eq_ignore_ascii_case(key) {
            return Some(value.trim().trim_matches('"').to_owned());
        }
    }
    None
}

fn pkce_verifier() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn open_browser(url: &str, configured_command: Option<&str>) -> Result<()> {
    let mut parts: Vec<&str> = configured_command
        .map(str::split_whitespace)
        .map(Iterator::collect)
        .unwrap_or_default();
    if parts.is_empty() {
        parts.push(default_browser_command()?);
    }
    let command = parts.remove(0);
    Command::new(command)
        .args(parts)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch browser command {command}"))?;
    Ok(())
}

fn default_browser_command() -> Result<&'static str> {
    #[cfg(target_os = "macos")]
    {
        Ok("open")
    }
    #[cfg(target_os = "linux")]
    {
        Ok("xdg-open")
    }
    #[cfg(target_os = "windows")]
    {
        Ok("cmd")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(anyhow!("no default browser command for this platform"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn pkce_challenge_matches_rfc7636_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn protected_resource_candidates_include_logdeck_root_fallback() {
        let endpoint = Url::parse("http://localhost:8080/mcp").unwrap();
        assert_eq!(
            protected_resource_metadata_candidates(&endpoint),
            vec![
                "http://localhost:8080/.well-known/oauth-protected-resource/mcp",
                "http://localhost:8080/.well-known/oauth-protected-resource",
            ]
        );
    }

    #[test]
    fn parses_resource_metadata_from_bearer_challenge() {
        let header = r#"Bearer resource_metadata="https://logdeck.example/.well-known/oauth-protected-resource", scope="mcp:read""#;
        assert_eq!(
            parse_www_authenticate_param(header, "resource_metadata").as_deref(),
            Some("https://logdeck.example/.well-known/oauth-protected-resource")
        );
    }

    #[cfg(unix)]
    #[test]
    fn login_completes_against_logdeck_shaped_oauth_server() {
        use std::os::unix::fs::PermissionsExt;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let (token_body_tx, token_body_rx) = mpsc::channel();
        let server_base = base_url.clone();
        let server = thread::spawn(move || {
            for _ in 0..8 {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_test_request(&mut stream);
                let first_line = request.lines().next().unwrap_or_default();
                let path = first_line.split_whitespace().nth(1).unwrap_or("/");

                match path.split('?').next().unwrap_or(path) {
                    "/mcp" => write_response(
                        &mut stream,
                        "401 Unauthorized",
                        &[(
                            "www-authenticate",
                            &format!(
                                "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\", scope=\"mcp:read\"",
                                server_base
                            ),
                        )],
                        "",
                    ),
                    "/.well-known/oauth-protected-resource" => write_response(
                        &mut stream,
                        "200 OK",
                        &[("content-type", "application/json")],
                        &format!(
                            r#"{{"resource":"{}/mcp","authorization_servers":["{}"],"scopes_supported":["mcp:read"]}}"#,
                            server_base, server_base
                        ),
                    ),
                    "/.well-known/oauth-authorization-server" => write_response(
                        &mut stream,
                        "200 OK",
                        &[("content-type", "application/json")],
                        &format!(
                            r#"{{"issuer":"{}","authorization_endpoint":"{}/oauth/authorize","token_endpoint":"{}/oauth/token","registration_endpoint":"{}/oauth/register","response_types_supported":["code"],"grant_types_supported":["authorization_code"],"token_endpoint_auth_methods_supported":["none"],"code_challenge_methods_supported":["S256"],"scopes_supported":["mcp:read"]}}"#,
                            server_base, server_base, server_base, server_base
                        ),
                    ),
                    "/oauth/register" => write_response(
                        &mut stream,
                        "201 Created",
                        &[("content-type", "application/json")],
                        r#"{"client_id":"test-client","redirect_uris":["http://127.0.0.1/callback"]}"#,
                    ),
                    "/oauth/authorize" => {
                        let url = Url::parse(&format!("{}{}", server_base, path)).unwrap();
                        let params: std::collections::HashMap<_, _> =
                            url.query_pairs().into_owned().collect();
                        assert_eq!(
                            params.get("client_id").map(String::as_str),
                            Some("test-client")
                        );
                        assert_eq!(
                            params.get("resource").map(String::as_str),
                            Some(format!("{}/mcp", server_base).as_str())
                        );
                        assert_eq!(
                            params.get("code_challenge_method").map(String::as_str),
                            Some("S256")
                        );
                        let redirect_uri = params.get("redirect_uri").unwrap();
                        let state = params.get("state").unwrap();
                        let location = format!("{redirect_uri}?code=test-code&state={state}");
                        write_response(&mut stream, "302 Found", &[("location", &location)], "");
                    }
                    "/oauth/token" => {
                        token_body_tx
                            .send(request.split("\r\n\r\n").nth(1).unwrap_or("").to_owned())
                            .unwrap();
                        write_response(
                            &mut stream,
                            "200 OK",
                            &[("content-type", "application/json")],
                            r#"{"access_token":"logdeck-token","token_type":"Bearer","expires_in":86400}"#,
                        );
                        break;
                    }
                    _ => write_response(&mut stream, "404 Not Found", &[], ""),
                }
            }
        });

        let dir = tempfile::TempDir::new().unwrap();
        let opened_url_path = dir.path().join("opened-url");
        let browser_script = dir.path().join("open-url.sh");
        std::fs::write(
            &browser_script,
            format!(
                "#!/bin/sh\nprintf '%s' \"$1\" > {}\n",
                opened_url_path.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&browser_script, std::fs::Permissions::from_mode(0o700)).unwrap();

        let login_base = base_url.clone();
        let browser_command = browser_script.to_string_lossy().to_string();
        let login_thread = thread::spawn(move || {
            login(OAuthLoginOptions {
                endpoint: format!("{login_base}/mcp"),
                browser_open_command: Some(browser_command),
                timeout: Duration::from_secs(5),
                client_name: "mcp2cli test".to_owned(),
            })
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        let opened_url = loop {
            if let Ok(value) = std::fs::read_to_string(&opened_url_path) {
                if !value.is_empty() {
                    break value;
                }
            }
            assert!(
                Instant::now() < deadline,
                "browser command did not receive URL"
            );
            thread::sleep(Duration::from_millis(25));
        };
        ureq::get(opened_url.trim()).call().unwrap();

        let result = login_thread.join().unwrap().unwrap();
        assert_eq!(result.access_token, "logdeck-token");
        assert_eq!(result.issuer, base_url);
        assert_eq!(result.expires_in, Some(86400));
        assert!(token_body_rx.recv().unwrap().contains("resource="));
        server.join().unwrap();
    }

    #[cfg(unix)]
    fn write_response(
        stream: &mut std::net::TcpStream,
        status: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) {
        let mut response = format!(
            "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        response.push_str(body);
        stream.write_all(response.as_bytes()).unwrap();
    }

    #[cfg(unix)]
    fn read_test_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut request = Vec::new();
        let mut buf = [0_u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
            if has_complete_http_request(&request) {
                break;
            }
        }
        String::from_utf8_lossy(&request).to_string()
    }

    #[cfg(unix)]
    fn has_complete_http_request(request: &[u8]) -> bool {
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        request.len() >= header_end + 4 + content_length
    }
}
