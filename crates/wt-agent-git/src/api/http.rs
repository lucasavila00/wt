use anyhow::{bail, Context, Result};
use graphql_client::{GraphQLQuery, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;
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
        let mut response = self
            .authorize(self.agent.post(self.url(path)))
            .send_json(&body)
            .context("send provider GraphQL request")?;
        let response: Response<Q::ResponseData> = response
            .body_mut()
            .read_json()
            .context("decode provider GraphQL response")?;
        if let Some(errors) = response.errors {
            let messages = errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            bail!("provider GraphQL error: {messages}");
        }
        response
            .data
            .ok_or_else(|| anyhow::anyhow!("provider GraphQL response has no data"))
    }

    pub(crate) fn read_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut response = self
            .authorize(self.agent.get(self.url(path)))
            .call()
            .context("send provider GET request")?;
        response
            .body_mut()
            .read_json()
            .context("decode provider response")
    }

    pub(crate) fn read_text(&self, path: &str) -> Result<String> {
        let mut response = self
            .authorize(self.agent.get(self.url(path)))
            .call()
            .context("send provider GET request")?;
        response
            .body_mut()
            .read_to_string()
            .context("read provider response")
    }

    pub(crate) fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let mut response = self
            .authorize(self.agent.post(self.url(path)))
            .send_json(body)
            .context("send provider POST request")?;
        response
            .body_mut()
            .read_json()
            .context("decode provider response")
    }

    pub(crate) fn post_without_body(&self, path: &str) -> Result<()> {
        self.authorize(self.agent.post(self.url(path)))
            .send_empty()
            .context("send provider POST request")?;
        Ok(())
    }

    fn authorize<T>(&self, request: ureq::RequestBuilder<T>) -> ureq::RequestBuilder<T> {
        let request = request
            .header("Accept", "application/json")
            .header("User-Agent", "wt-agent-git");
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
