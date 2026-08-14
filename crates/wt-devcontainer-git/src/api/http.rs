use anyhow::{bail, Context, Result};
use graphql_client::{GraphQLQuery, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::Read;
use std::time::Duration;

#[derive(Clone, Copy)]
pub(crate) enum ProviderAuthentication {
    Github,
    Gitlab,
}

pub(crate) struct ProviderHttpClient {
    agent: ureq::Agent,
    base: String,
    token: String,
    authentication: ProviderAuthentication,
}

impl ProviderHttpClient {
    pub(crate) fn new(
        base: String,
        token: &str,
        authentication: ProviderAuthentication,
    ) -> Result<Self> {
        let base = base.trim_end_matches('/').to_owned();
        let parsed = url::Url::parse(&base).context("parse provider API URL")?;
        if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
            bail!("provider API URL must be HTTP or HTTPS");
        }
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .build();
        Ok(Self {
            agent: ureq::Agent::new_with_config(config),
            base,
            token: token.to_owned(),
            authentication,
        })
    }

    pub(crate) fn execute_graphql<Q>(
        &self,
        path: &str,
        variables: Q::Variables,
    ) -> Result<Q::ResponseData>
    where
        Q: GraphQLQuery,
        Q::Variables: Serialize,
        Q::ResponseData: DeserializeOwned,
    {
        let body = Q::build_query(variables);
        let url = self.url(path);
        let response = self
            .authorize(self.agent.post(&url))
            .send_json(&body)
            .with_context(|| connection_context("POST", &url))?;
        let body = read_response(response, "POST", &url)?;
        let response: Response<Q::ResponseData> = decode_json(&body, &url, "GraphQL")?;
        if let Some(errors) = response.errors {
            let messages = errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            bail!(
                "provider GraphQL request failed\nEndpoint: {url}\nProvider response: {messages}\nNext step: use the provider response to correct the command, credential, or repository configuration"
            );
        }
        response
            .data
            .ok_or_else(|| anyhow::anyhow!("provider GraphQL response has no data"))
    }

    pub(crate) fn read_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        let response = self
            .authorize(self.agent.get(&url))
            .call()
            .with_context(|| connection_context("GET", &url))?;
        let body = read_response(response, "GET", &url)?;
        decode_json(&body, &url, "JSON")
    }

    pub(crate) fn read_text(&self, path: &str) -> Result<String> {
        let url = self.url(path);
        let response = self
            .authorize(self.agent.get(&url))
            .call()
            .with_context(|| connection_context("GET", &url))?;
        read_response(response, "GET", &url)
    }

    pub(crate) fn read_optional_text(&self, path: &str) -> Result<Option<String>> {
        let url = self.url(path);
        let response = self
            .authorize(self.agent.get(&url))
            .call()
            .with_context(|| connection_context("GET", &url))?;
        if response.status() == ureq::http::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        read_response(response, "GET", &url).map(Some)
    }

    pub(crate) fn post_without_body(&self, path: &str) -> Result<()> {
        let url = self.url(path);
        let response = self
            .authorize(self.agent.post(&url))
            .send_empty()
            .with_context(|| connection_context("POST", &url))?;
        read_response(response, "POST", &url)?;
        Ok(())
    }

    fn authorize<T>(&self, request: ureq::RequestBuilder<T>) -> ureq::RequestBuilder<T> {
        let request = request
            .header("Accept", "application/json")
            .header("User-Agent", "wt-devcontainer-git");
        match self.authentication {
            ProviderAuthentication::Github => request
                .header("Authorization", &format!("Bearer {}", self.token))
                .header("X-GitHub-Api-Version", "2022-11-28"),
            ProviderAuthentication::Gitlab => request.header("PRIVATE-TOKEN", &self.token),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }
}

fn connection_context(method: &str, url: &str) -> String {
    format!(
        "could not reach provider API\nHTTP: {method} {url}\nNext step: check DNS, TLS, and outbound provider access from the WT host"
    )
}

fn decode_json<T: DeserializeOwned>(body: &str, url: &str, kind: &str) -> Result<T> {
    serde_json::from_str(body).with_context(|| {
        format!(
            "decode provider {kind} response from {url}\nProvider response: {}",
            truncate(body, 4096)
        )
    })
}

fn read_response(
    mut response: ureq::http::Response<ureq::Body>,
    method: &str,
    url: &str,
) -> Result<String> {
    let status = response.status();
    let metadata = response_metadata(&response);
    let body: Result<String> = if status.is_success() {
        response
            .body_mut()
            .read_to_string()
            .map_err(anyhow::Error::from)
    } else {
        let mut body = String::new();
        response
            .body_mut()
            .as_reader()
            .take(4097)
            .read_to_string(&mut body)
            .map(|_| body)
            .map_err(anyhow::Error::from)
    };
    let body = body.with_context(|| format!("read provider response from {url}"))?;
    if status.is_success() {
        return Ok(body);
    }

    let hint = match status.as_u16() {
        401 => "The installed API credential is invalid or expired. Reinstall the gateway credential.",
        403 => "The installed API credential lacks permission, or the provider has rate-limited it. Check the credential and provider response.",
        404 => "The project or object was not found with this credential. Check its repository access.",
        429 => "The provider rate limit was reached. Wait and retry this command.",
        _ => "The provider rejected the request. Use its response below to correct the problem.",
    };
    let body = if body.trim().is_empty() {
        "<empty response>".to_owned()
    } else {
        truncate(&body, 4096)
    };
    bail!(
        "provider API request failed\nHTTP: {method} {url} -> {}{metadata}\nProvider response: {body}\nNext step: {hint}",
        status.as_u16()
    )
}

fn response_metadata(response: &ureq::http::Response<ureq::Body>) -> String {
    const HEADERS: [&str; 6] = [
        "retry-after",
        "x-ratelimit-remaining",
        "x-ratelimit-reset",
        "x-ratelimit-resource",
        "x-github-request-id",
        "x-request-id",
    ];
    let values = HEADERS
        .into_iter()
        .filter_map(|name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| format!("\n{name}: {value}"))
        })
        .collect::<String>();
    values
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        value.to_owned()
    } else {
        format!("{}…", &value[..value.floor_char_boundary(limit)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_server::{serve_one_with_status, ExpectedRequest};

    #[test]
    fn http_errors_preserve_status_provider_body_and_recovery_hint() {
        let (base_url, server) = serve_one_with_status(
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"message":"Resource not accessible by token"}"#,
            },
            403,
        );
        let client = ProviderHttpClient::new(
            base_url.clone(),
            "fixture-token",
            ProviderAuthentication::Github,
        )
        .unwrap();

        let error = client
            .read_json::<serde_json::Value>("repos/acme/widget")
            .unwrap_err();
        let message = format!("{error:#}").replace(&base_url, "<provider>");

        insta::assert_snapshot!(message, @r###"
        provider API request failed
        HTTP: GET <provider>/repos/acme/widget -> 403
        Provider response: {"message":"Resource not accessible by token"}
        Next step: The installed API credential lacks permission, or the provider has rate-limited it. Check the credential and provider response.
        "###);
        server.join().unwrap().unwrap();
    }
}
