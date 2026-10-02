use crate::models::*;
use reqwest::{
    blocking::{Client, Response},
    header::{ACCEPT, USER_AGENT},
};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct GitHub {
    client: Client,
    base: String,
}
impl GitHub {
    pub fn new() -> Result<Self> {
        Self::with_base("https://api.github.com".into())
    }
    pub fn with_base(base: String) -> Result<Self> {
        Ok(Self {
            base,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| network())?,
        })
    }
    fn request(
        &self,
        method: reqwest::Method,
        route: &str,
        token: &str,
        body: Option<Value>,
    ) -> Result<Response> {
        let mut req = self
            .client
            .request(method, format!("{}{}", self.base, route))
            .header(USER_AGENT, "lazysync/0.1")
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .bearer_auth(token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().map_err(|_| network())?;
        if res.status().is_success() {
            return Ok(res);
        }
        let code = res.status().as_u16();
        let rate = res
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            == Some("0")
            || code == 429;
        Err(if rate {
            AppError::new(
                "rateLimit",
                "GitHub rate limit reached.",
                "Wait for the rate limit to reset, then retry.",
            )
        } else {
            match code {
                401 => AppError::new(
                    "authentication",
                    "GitHub rejected this token.",
                    "Replace the expired or revoked token.",
                ),
                403 | 404 => AppError::new(
                    "permission",
                    "Token cannot access this repository or action.",
                    "Grant repository access and write permissions, then replace the token.",
                ),
                422 => AppError::new(
                    "validation",
                    "GitHub rejected the repository name or it already exists.",
                    "Choose another name or connect the existing repository.",
                ),
                _ => AppError::new(
                    "github",
                    format!("GitHub request failed (HTTP {code})."),
                    "Retry when GitHub is available.",
                ),
            }
        })
    }
    pub fn identity(&self, token: &str) -> Result<Identity> {
        let v: Value = self
            .request(reqwest::Method::GET, "/user", token, None)?
            .json()
            .map_err(|_| network())?;
        let login = v["login"].as_str().unwrap_or_default().to_string();
        if login.is_empty() {
            return Err(network());
        }
        Ok(Identity {
            name: v["name"].as_str().unwrap_or(&login).into(),
            email: v["email"].as_str().map(String::from).unwrap_or_else(|| {
                format!(
                    "{}+{}@users.noreply.github.com",
                    v["id"].as_u64().unwrap_or(0),
                    login
                )
            }),
            login,
        })
    }
    pub fn repositories(&self, token: &str, page: u32) -> Result<Vec<RemoteRepository>> {
        let v: Vec<Value> = self.request(reqwest::Method::GET, &format!("/user/repos?per_page=30&page={}&sort=updated&affiliation=owner,collaborator,organization_member", page.clamp(1, 10000)), token, None)?.json().map_err(|_| network())?;
        v.iter().map(parse_repo).collect()
    }
    pub fn repository(&self, token: &str, name: &str) -> Result<RemoteRepository> {
        validate_full_name(name)?;
        let v: Value = self
            .request(reqwest::Method::GET, &format!("/repos/{name}"), token, None)?
            .json()
            .map_err(|_| network())?;
        parse_repo(&v)
    }
    pub fn create_private(
        &self,
        token: &str,
        name: &str,
        description: &str,
    ) -> Result<RemoteRepository> {
        if name.is_empty()
            || name.len() > 100
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(AppError::new(
                "validation",
                "Use a repository name with letters, numbers, dots, underscores, or hyphens.",
                "Edit the repository name.",
            ));
        }
        let v: Value = self
            .request(
                reqwest::Method::POST,
                "/user/repos",
                token,
                Some(
                    json!({"name":name,"description":description,"private":true,"auto_init":false}),
                ),
            )?
            .json()
            .map_err(|_| network())?;
        parse_repo(&v)
    }
}
pub fn validate_full_name(name: &str) -> Result<()> {
    let p: Vec<_> = name.split('/').collect();
    if p.len() != 2
        || p.iter().any(|s| {
            s.is_empty()
                || *s == "."
                || *s == ".."
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
    {
        return Err(AppError::new(
            "repository",
            "Invalid GitHub repository name.",
            "Choose a repository from GitHub.",
        ));
    }
    Ok(())
}
fn parse_repo(v: &Value) -> Result<RemoteRepository> {
    let full_name = v["full_name"].as_str().ok_or_else(network)?.to_string();
    validate_full_name(&full_name)?;
    Ok(RemoteRepository {
        id: v["id"].as_u64().ok_or_else(network)?.to_string(),
        owner: v["owner"]["login"].as_str().unwrap_or_default().into(),
        full_name,
        private: v["private"].as_bool().unwrap_or(true),
        writable: v["permissions"]["push"].as_bool().unwrap_or(false),
        default_branch: v["default_branch"].as_str().unwrap_or("main").into(),
    })
}
fn network() -> AppError {
    AppError::new(
        "network",
        "Could not reach GitHub or read its response.",
        "Check your connection and retry.",
    )
}
