pub mod model;
mod timeutil;

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use model::{CiCheck, CiCheckState, CiState, MergeBlockers, Pr, ReviewState, StatusContextNode};
use octocrab::Octocrab;
use timeutil::{parse_github_datetime_to_unix, unix_to_ymd};

#[derive(Debug, serde::Serialize)]
struct PaginationVars {
    page_size: i32,
    cursor: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct GraphQlPayload<V> {
    query: &'static str,
    variables: V,
}

#[derive(Debug, serde::Deserialize)]
struct PageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RepoOwner {
    login: String,
}

#[derive(Debug, serde::Deserialize)]
struct Repository {
    name: String,
    owner: RepoOwner,
}

#[derive(Debug, serde::Deserialize)]
struct Author {
    login: String,
}

#[derive(Debug, serde::Deserialize)]
struct ReviewRequestConnection {
    nodes: Option<Vec<ReviewRequestNode>>,
}

#[derive(Debug, serde::Deserialize)]
struct ReviewRequestNode {
    #[serde(rename = "requestedReviewer")]
    requested_reviewer: Option<RequestedReviewer>,
}

#[derive(Debug, serde::Deserialize)]
struct RequestedReviewer {
    #[serde(rename = "__typename")]
    typename: Option<String>,
    login: Option<String>, // User
}

#[derive(Debug, serde::Deserialize)]
struct StatusCheckRollup {
    state: Option<String>,
    contexts: Option<StatusContexts>,
}

#[derive(Debug, serde::Deserialize)]
struct StatusContexts {
    nodes: Option<Vec<StatusContextNode>>,
}

#[derive(Debug, serde::Deserialize)]
struct CommitInner {
    #[serde(rename = "statusCheckRollup")]
    status_check_rollup: Option<StatusCheckRollup>,
}

#[derive(Debug, serde::Deserialize)]
struct CommitNode {
    commit: Option<CommitInner>,
}

#[derive(Debug, serde::Deserialize)]
struct Commits {
    nodes: Option<Vec<CommitNode>>,
}

#[derive(Debug, serde::Deserialize)]
struct ReviewsConnection {
    #[serde(rename = "totalCount")]
    total_count: Option<i32>,
}

#[derive(Debug, serde::Deserialize)]
struct BranchProtectionRule {
    #[serde(rename = "requiredApprovingReviewCount")]
    required_approving_review_count: Option<i32>,
    #[serde(rename = "requiredStatusCheckContexts")]
    required_status_check_contexts: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct BaseRef {
    #[serde(rename = "branchProtectionRule")]
    branch_protection_rule: Option<BranchProtectionRule>,
}

#[derive(Debug, serde::Deserialize)]
struct PullRequestNode {
    number: i64,
    title: String,
    url: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    repository: Repository,
    author: Option<Author>,
    #[serde(rename = "reviewRequests")]
    review_requests: Option<ReviewRequestConnection>,
    #[serde(rename = "headRefOid")]
    head_ref_oid: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    #[serde(rename = "isDraft")]
    is_draft: Option<bool>,
    mergeable: Option<String>,
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    commits: Option<Commits>,
    reviews: Option<ReviewsConnection>,
    #[serde(rename = "baseRef")]
    base_ref: Option<BaseRef>,
}

#[derive(Debug, serde::Deserialize)]
struct ViewerPullRequests {
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
    nodes: Option<Vec<PullRequestNode>>,
}

#[derive(Debug, serde::Deserialize)]
struct Viewer {
    login: String,
    #[serde(rename = "pullRequests")]
    pull_requests: ViewerPullRequests,
}

#[derive(Debug, serde::Deserialize)]
struct AuthoredData {
    viewer: Viewer,
}

#[derive(Debug, serde::Deserialize)]
struct GraphQlResponse<T> {
    data: T,
}

#[derive(Debug, serde::Deserialize)]
struct SearchResult {
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
    nodes: Option<Vec<SearchNode>>,
}

#[derive(Debug, serde::Deserialize)]
struct SearchNode {
    #[serde(rename = "__typename")]
    typename: Option<String>,
    number: Option<i64>,
    title: Option<String>,
    url: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    repository: Option<Repository>,
    author: Option<Author>,
    #[serde(rename = "reviewRequests")]
    review_requests: Option<ReviewRequestConnection>,
    #[serde(rename = "headRefOid")]
    head_ref_oid: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    #[serde(rename = "isDraft")]
    is_draft: Option<bool>,
    mergeable: Option<String>,
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: Option<String>,
    commits: Option<Commits>,
    reviews: Option<ReviewsConnection>,
    #[serde(rename = "baseRef")]
    base_ref: Option<BaseRef>,
}

impl SearchNode {
    fn into_pull_request(self) -> Option<PullRequestNode> {
        if self.typename.as_deref()? != "PullRequest" {
            return None;
        }
        Some(PullRequestNode {
            number: self.number?,
            title: self.title?,
            url: self.url?,
            updated_at: self.updated_at?,
            repository: self.repository?,
            author: self.author,
            review_requests: self.review_requests,
            head_ref_oid: self.head_ref_oid,
            review_decision: self.review_decision,
            is_draft: self.is_draft,
            mergeable: self.mergeable,
            merge_state_status: self.merge_state_status,
            commits: self.commits,
            reviews: self.reviews,
            base_ref: self.base_ref,
        })
    }
}

#[derive(Debug, serde::Deserialize)]
struct SearchData {
    search: SearchResult,
}

const AUTHORED_QUERY: &str = r#"
query ($page_size: Int!, $cursor: String) {
  viewer {
    login
    pullRequests(states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}, first: $page_size, after: $cursor) {
      pageInfo {
        hasNextPage
        endCursor
      }
      nodes {
        ...PrFields
      }
    }
  }
}

fragment PrFields on PullRequest {
  number
  title
  url
  updatedAt
  repository {
    name
    owner {
      login
    }
  }
  author {
    login
  }
  reviewRequests(first: 20) {
    nodes {
      requestedReviewer {
        __typename
        ... on User {
          login
        }
      }
    }
  }
  headRefOid
  reviewDecision
  isDraft
  mergeable
  mergeStateStatus
  commits(last: 1) {
    nodes {
      commit {
        statusCheckRollup {
          state
          contexts(first: 50) {
            nodes {
              __typename
              ... on CheckRun {
                name
                conclusion
                detailsUrl
                startedAt
              }
              ... on StatusContext {
                context
                state
                targetUrl
              }
            }
          }
        }
      }
    }
  }
  reviews(states: APPROVED) {
    totalCount
  }
  baseRef {
    branchProtectionRule {
      requiredApprovingReviewCount
      requiredStatusCheckContexts
    }
  }
}
"#;

const REVIEW_REQUESTED_QUERY: &str = r#"
query ($page_size: Int!, $cursor: String, $search_query: String!) {
  search(query: $search_query, type: ISSUE, first: $page_size, after: $cursor) {
    pageInfo {
      hasNextPage
      endCursor
    }
    nodes {
      __typename
      ... on PullRequest {
        number
        title
        url
        updatedAt
        repository {
          name
          owner {
            login
          }
        }
        author {
          login
        }
        reviewRequests(first: 20) {
          nodes {
            requestedReviewer {
              __typename
              ... on User {
                login
              }
            }
          }
        }
        headRefOid
        reviewDecision
        isDraft
        mergeable
        mergeStateStatus
        commits(last: 1) {
          nodes {
            commit {
              statusCheckRollup {
                state
                contexts(first: 50) {
                  nodes {
                    __typename
                    ... on CheckRun {
                      name
                      conclusion
                      detailsUrl
                      startedAt
                    }
                    ... on StatusContext {
                      context
                      state
                      targetUrl
                    }
                  }
                }
              }
            }
          }
        }
        reviews(states: APPROVED) {
          totalCount
        }
        baseRef {
          branchProtectionRule {
            requiredApprovingReviewCount
            requiredStatusCheckContexts
          }
        }
      }
    }
  }
}
"#;

fn rollup_state(node: &PullRequestNode) -> Option<&str> {
    node.commits
        .as_ref()?
        .nodes
        .as_ref()?
        .first()?
        .commit
        .as_ref()?
        .status_check_rollup
        .as_ref()?
        .state
        .as_deref()
}

fn status_context_nodes(node: &PullRequestNode) -> Vec<StatusContextNode> {
    node.commits
        .as_ref()
        .and_then(|c| c.nodes.as_ref())
        .and_then(|nodes| nodes.first())
        .and_then(|n| n.commit.as_ref())
        .and_then(|c| c.status_check_rollup.as_ref())
        .and_then(|s| s.contexts.as_ref())
        .and_then(|c| c.nodes.as_ref())
        .cloned()
        .unwrap_or_default()
}

fn map_ci_checks(node: &PullRequestNode) -> Vec<CiCheck> {
    let mut out = Vec::new();
    for ctx in status_context_nodes(node) {
        match ctx.typename.as_deref() {
            Some("CheckRun") => {
                let name = ctx.name.unwrap_or_else(|| "check".to_string());
                let started_at_unix = ctx
                    .started_at
                    .as_ref()
                    .and_then(|s| parse_github_datetime_to_unix(s));
                let state = match ctx.conclusion.as_deref() {
                    Some("SUCCESS") => CiCheckState::Success,
                    Some("FAILURE") | Some("CANCELLED") => CiCheckState::Failure,
                    Some("NEUTRAL") | Some("SKIPPED") => CiCheckState::Neutral,
                    _ => CiCheckState::Running,
                };
                let url = ctx.details_url.or(ctx.target_url);
                out.push(CiCheck {
                    name,
                    state,
                    url,
                    started_at_unix,
                });
            }
            Some("StatusContext") => {
                let name = ctx.context.unwrap_or_else(|| "status".to_string());
                let state = match ctx.state.as_deref() {
                    Some("SUCCESS") => CiCheckState::Success,
                    Some("FAILURE") => CiCheckState::Failure,
                    Some("PENDING") => CiCheckState::Running,
                    _ => CiCheckState::None,
                };
                let url = ctx.target_url;
                out.push(CiCheck {
                    name,
                    state,
                    url,
                    started_at_unix: None,
                });
            }
            _ => {}
        }
    }
    out
}

fn derive_ci_state(rollup: Option<&str>, checks: &[CiCheck]) -> CiState {
    if checks
        .iter()
        .any(|c| matches!(c.state, CiCheckState::Running))
    {
        return CiState::Running;
    }
    if checks
        .iter()
        .any(|c| matches!(c.state, CiCheckState::Failure))
    {
        return CiState::Failure;
    }
    if checks
        .iter()
        .any(|c| matches!(c.state, CiCheckState::Success))
    {
        return CiState::Success;
    }

    match rollup.unwrap_or("none") {
        "SUCCESS" => CiState::Success,
        "FAILURE" => CiState::Failure,
        "PENDING" | "IN_PROGRESS" => CiState::Running,
        _ => CiState::None,
    }
}

fn map_review_state(node: &PullRequestNode, is_requested: bool) -> ReviewState {
    if is_requested {
        return ReviewState::Requested;
    }
    match node.review_decision.as_deref() {
        Some("APPROVED") => ReviewState::Approved,
        _ => ReviewState::None,
    }
}

fn is_review_requested_by_user(node: &PullRequestNode, viewer_login: &str) -> bool {
    let Some(rr) = node.review_requests.as_ref() else {
        return false;
    };
    let Some(nodes) = rr.nodes.as_ref() else {
        return false;
    };
    for n in nodes {
        let Some(r) = n.requested_reviewer.as_ref() else {
            continue;
        };
        if r.typename.as_deref() == Some("User") && r.login.as_deref() == Some(viewer_login) {
            return true;
        }
    }
    false
}

fn compute_merge_blockers(node: &PullRequestNode, ci_checks: &[CiCheck]) -> MergeBlockers {
    let has_conflicts = node
        .mergeable
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("CONFLICTING"));

    let is_behind_base = node
        .merge_state_status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("BEHIND"));

    let (required_approvals, required_checks) = node
        .base_ref
        .as_ref()
        .and_then(|br| br.branch_protection_rule.as_ref())
        .map(|bpr| {
            let approvals = bpr.required_approving_review_count.map(|c| c as u32);
            let checks = bpr
                .required_status_check_contexts
                .clone()
                .unwrap_or_default();
            (approvals, checks)
        })
        .unwrap_or((None, Vec::new()));

    let current_approvals = node
        .reviews
        .as_ref()
        .and_then(|r| r.total_count)
        .unwrap_or(0) as u32;

    let check_names_success: std::collections::HashSet<_> = ci_checks
        .iter()
        .filter(|c| matches!(c.state, CiCheckState::Success))
        .map(|c| c.name.as_str())
        .collect();

    let failing_required_checks: Vec<String> = required_checks
        .iter()
        .filter(|name| !check_names_success.contains(name.as_str()))
        .cloned()
        .collect();

    MergeBlockers {
        has_conflicts,
        required_approvals,
        current_approvals,
        required_checks,
        failing_required_checks,
        is_behind_base,
    }
}

fn to_pr(node: PullRequestNode, is_requested: bool, viewer_login: &str) -> Option<Pr> {
    let ci_checks = map_ci_checks(&node);
    let ci_state = derive_ci_state(rollup_state(&node), &ci_checks);
    let last_commit_sha = node.head_ref_oid.clone();
    let review_state = map_review_state(&node, is_requested);
    let owner = node.repository.owner.login.clone();
    let repo = node.repository.name.clone();
    let author = node
        .author
        .as_ref()
        .map(|a| a.login.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let updated_at_unix = parse_github_datetime_to_unix(&node.updated_at)?;
    let pr_key = format!("{owner}/{repo}#{}", node.number);

    let is_viewer_author = node
        .author
        .as_ref()
        .map(|a| a.login.as_str() == viewer_login)
        .unwrap_or(false);

    let merge_blockers = compute_merge_blockers(&node, &ci_checks);
    let merge_blockers = if merge_blockers.is_clear() {
        None
    } else {
        Some(merge_blockers)
    };

    Some(Pr {
        pr_key,
        owner,
        repo,
        number: node.number,
        author,
        title: node.title,
        url: node.url,
        updated_at_unix,
        last_commit_sha,
        ci_state,
        ci_checks,
        review_state,
        is_draft: node.is_draft.unwrap_or(false),
        mergeable: node.mergeable.clone(),
        merge_state_status: node.merge_state_status.clone(),
        is_viewer_author,
        merge_blockers,
    })
}

fn merge_into(map: &mut HashMap<String, Pr>, mut pr: Pr) {
    if let Some(existing) = map.get(&pr.pr_key) && existing.is_viewer_author {
        pr.is_viewer_author = true;
    }
    map.insert(pr.pr_key.clone(), pr);
}

pub async fn fetch_attention_prs(
    octo: &Octocrab,
    cutoff_ts: i64,
    include_team_requests: bool,
) -> Result<Vec<Pr>> {
    let mut authored: Vec<PullRequestNode> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut viewer_login: Option<String> = None;
    loop {
        let vars = PaginationVars {
            page_size: 50,
            cursor: cursor.clone(),
        };
        let payload = GraphQlPayload {
            query: AUTHORED_QUERY,
            variables: vars,
        };
        let resp: GraphQlResponse<AuthoredData> = octo
            .graphql(&payload)
            .await
            .map_err(|e| anyhow!("GitHub GraphQL authored query failed: {e:?}"))?;

        if viewer_login.is_none() {
            viewer_login = Some(resp.data.viewer.login.clone());
        }

        if let Some(nodes) = resp.data.viewer.pull_requests.nodes {
            let mut keep = Vec::new();
            let mut min_updated: Option<i64> = None;
            for n in nodes {
                if let Some(u) = parse_github_datetime_to_unix(&n.updated_at) {
                    min_updated = Some(min_updated.map(|m| m.min(u)).unwrap_or(u));
                    if u >= cutoff_ts {
                        keep.push(n);
                    }
                }
            }
            authored.extend(keep);
            if min_updated.is_some_and(|m| m < cutoff_ts) {
                break;
            }
        }
        let pi = resp.data.viewer.pull_requests.page_info;
        if !pi.has_next_page {
            break;
        }
        cursor = pi.end_cursor;
        if cursor.is_none() {
            break;
        }
    }

    let viewer_login = viewer_login.unwrap_or_else(|| "unknown".to_string());

    let cutoff_date = unix_to_ymd(cutoff_ts)
        .map(|(y, m, d)| format!("{y:04}-{m:02}-{d:02}"))
        .unwrap_or_else(|| "1970-01-01".to_string());
    let search_query = format!(
        "is:pr is:open review-requested:@me sort:updated-desc updated:>={}",
        cutoff_date
    );

    let mut requested_nodes: Vec<PullRequestNode> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        #[derive(Debug, serde::Serialize)]
        struct SearchVars {
            page_size: i32,
            cursor: Option<String>,
            search_query: String,
        }

        let vars = SearchVars {
            page_size: 50,
            cursor: cursor.clone(),
            search_query: search_query.clone(),
        };
        let payload = GraphQlPayload {
            query: REVIEW_REQUESTED_QUERY,
            variables: vars,
        };
        let resp: GraphQlResponse<SearchData> = octo
            .graphql(&payload)
            .await
            .map_err(|e| anyhow!("GitHub GraphQL review-requested query failed: {e:?}"))?;

        if let Some(nodes) = resp.data.search.nodes {
            let mut min_updated: Option<i64> = None;
            for n in nodes {
                if let Some(pr) = n.into_pull_request() {
                    if let Some(u) = parse_github_datetime_to_unix(&pr.updated_at) {
                        min_updated = Some(min_updated.map(|m| m.min(u)).unwrap_or(u));
                        if u < cutoff_ts {
                            continue;
                        }
                    }
                    if include_team_requests || is_review_requested_by_user(&pr, &viewer_login) {
                        requested_nodes.push(pr);
                    }
                }
            }
            if min_updated.is_some_and(|m| m < cutoff_ts) {
                break;
            }
        }
        let pi = resp.data.search.page_info;
        if !pi.has_next_page {
            break;
        }
        cursor = pi.end_cursor;
        if cursor.is_none() {
            break;
        }
    }

    let mut by_key: HashMap<String, Pr> = HashMap::new();

    for node in authored {
        let requested_user = is_review_requested_by_user(&node, &viewer_login);
        if let Some(mut pr) = to_pr(node, requested_user, &viewer_login) {
            pr.is_viewer_author = true;
            merge_into(&mut by_key, pr);
        }
    }

    for node in requested_nodes {
        if let Some(pr) = to_pr(node, true, &viewer_login) {
            merge_into(&mut by_key, pr);
        }
    }

    Ok(by_key.into_values().collect())
}

/// Synchronous facade that owns its own Tokio runtime.
pub fn fetch_attention_prs_sync(
    token: &str,
    api_base: Option<String>,
    cutoff_ts: i64,
    include_team_requests: bool,
) -> Result<Vec<Pr>> {
    let token = token.to_owned();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow!("failed to build tokio runtime: {e}"))?;

    rt.block_on(async move {
        let mut builder = Octocrab::builder().personal_token(token);
        if let Some(api) = api_base {
            builder = builder
                .base_uri(api)
                .map_err(|e| anyhow!("invalid GITHUB_API_URL: {e}"))?;
        }
        let octo = builder
            .build()
            .map_err(|e| anyhow!("failed to init GitHub client: {e}"))?;
        fetch_attention_prs(&octo, cutoff_ts, include_team_requests).await
    })
}
