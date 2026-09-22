use reqwest::redirect::Policy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::db::{Desk, QuestionType};
use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize)]
struct DeskView {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    question_types: Vec<RawQuestionType>,
}

#[derive(Debug, Deserialize)]
struct RawQuestionType {
    id: i64,
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DesksResponse {
    List(Vec<Desk>),
    Wrapped { desks: Vec<Desk> },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DeskViewResponse {
    Direct(DeskView),
    Wrapped { desk: DeskView },
}

#[derive(Debug, Deserialize)]
struct ApiStatus {
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct TallyPayload<'a> {
    desk_id: i64,
    question_type_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<&'a str>,
}

pub fn client_for(base_url: &str) -> AppResult<Client> {
    client_builder(base_url, true)?.build().map_err(Into::into)
}

fn post_client_for(base_url: &str) -> AppResult<Client> {
    client_builder(base_url, false)?.build().map_err(Into::into)
}

fn client_builder(base_url: &str, follow_redirects: bool) -> AppResult<reqwest::ClientBuilder> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .user_agent("StatTracker/0.1");
    if !follow_redirects {
        builder = builder.redirect(Policy::none());
    }
    if accepts_invalid_certs(base_url) {
        builder = builder.danger_accept_invalid_certs(true);
    }
    Ok(builder)
}

fn accepts_invalid_certs(url: &str) -> bool {
    match reqwest::Url::parse(url) {
        Ok(parsed) => parsed
            .host_str()
            .map(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host.ends_with(".loc")
                    || host.ends_with(".local")
                    || host.ends_with(".test")
                    || host == "127.0.0.1"
                    || host == "::1"
            })
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn join_url(base: &str, route: &str) -> String {
    let base = if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    };
    format!("{base}{route}")
}

pub async fn fetch_desks(base_url: &str) -> AppResult<Vec<Desk>> {
    let client = client_for(base_url)?;
    let url = join_url(base_url, "desks/index.json");
    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        return Err(AppError::Message(format!(
            "Could not load desks (HTTP {}).",
            response.status()
        )));
    }
    let body = response.text().await?;
    let parsed: DesksResponse = serde_json::from_str(&body).map_err(|_| {
        AppError::Message("The desks list was not valid JSON. Check the base URL.".into())
    })?;
    Ok(match parsed {
        DesksResponse::List(list) => list,
        DesksResponse::Wrapped { desks } => desks,
    })
}

pub async fn fetch_question_types(base_url: &str, desk_id: i64) -> AppResult<Vec<QuestionType>> {
    let client = client_for(base_url)?;
    let url = join_url(base_url, &format!("desks/view/{desk_id}.json"));
    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        return Err(AppError::Message(format!(
            "Could not load question types (HTTP {}).",
            response.status()
        )));
    }
    let body = response.text().await?;
    let parsed: DeskViewResponse = serde_json::from_str(&body).map_err(|_| {
        AppError::Message("The desk view response was not valid JSON.".into())
    })?;
    let view = match parsed {
        DeskViewResponse::Direct(view) => view,
        DeskViewResponse::Wrapped { desk } => desk,
    };
    let _ = (view.id, view.name);
    Ok(view
        .question_types
        .into_iter()
        .enumerate()
        .map(|(index, item)| QuestionType {
            id: item.id,
            name: item.name,
            description: item.description,
            sort_order: index as i64,
        })
        .collect())
}

fn extract_api_status(body: &str) -> Option<ApiStatus> {
    if let Ok(parsed) = serde_json::from_str::<ApiStatus>(body) {
        return Some(parsed);
    }
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    if start >= end {
        return None;
    }
    serde_json::from_str(&body[start..=end]).ok()
}

fn tally_post_failed(status: reqwest::StatusCode, parsed: Option<&ApiStatus>) -> Option<String> {
    if parsed.and_then(|value| value.success) == Some(true) {
        return None;
    }
    if parsed.and_then(|value| value.success) == Some(false) {
        return Some(
            parsed
                .and_then(|value| value.message.clone())
                .unwrap_or_else(|| "The server reported that the tally was not saved.".into()),
        );
    }
    if status.is_success() {
        return None;
    }
    Some(
        parsed
            .and_then(|value| value.message.clone())
            .unwrap_or_else(|| format!("The server rejected a tally (HTTP {status}).")),
    )
}

pub async fn post_tally(
    base_url: &str,
    desk_id: i64,
    question_type_id: i64,
    created: Option<&str>,
) -> AppResult<()> {
    let client = post_client_for(base_url)?;
    let url = join_url(base_url, "question-tallies/add");
    let response = client
        .post(&url)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Accept", "application/json")
        .json(&TallyPayload {
            desk_id,
            question_type_id,
            created,
        })
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let parsed = extract_api_status(&body);
    if let Some(detail) = tally_post_failed(status, parsed.as_ref()) {
        return Err(AppError::Message(detail));
    }
    Ok(())
}
