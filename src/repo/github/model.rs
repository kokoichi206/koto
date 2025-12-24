#[derive(Debug, Clone)]
pub enum CiState {
    Success,
    Failure,
    Running,
    None,
}

#[derive(Debug, Clone)]
pub enum ReviewState {
    Requested,
    Approved,
    None,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CiCheckState {
    Success,
    Failure,
    Running,
    Neutral,
    None,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CiCheck {
    pub name: String,
    pub state: CiCheckState,
    pub url: Option<String>,
    pub started_at_unix: Option<i64>,
}

/// Detailed information about why a PR cannot be merged.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MergeBlockers {
    /// PR has merge conflicts with the base branch.
    pub has_conflicts: bool,
    /// Number of approving reviews required by branch protection.
    pub required_approvals: Option<u32>,
    /// Current number of approving reviews.
    pub current_approvals: u32,
    /// Required status check contexts from branch protection.
    pub required_checks: Vec<String>,
    /// Subset of required checks that are failing or missing.
    pub failing_required_checks: Vec<String>,
    /// Base branch is ahead of the PR branch.
    pub is_behind_base: bool,
}

impl MergeBlockers {
    /// Returns true if there are no merge blockers.
    pub fn is_clear(&self) -> bool {
        !self.has_conflicts
            && !self.is_behind_base
            && self.failing_required_checks.is_empty()
            && self
                .required_approvals
                .map(|r| self.current_approvals >= r)
                .unwrap_or(true)
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StatusContextNode {
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
    // CheckRun
    pub name: Option<String>,
    pub conclusion: Option<String>,
    #[serde(rename = "detailsUrl")]
    pub details_url: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: Option<String>,
    // StatusContext
    pub context: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "targetUrl")]
    pub target_url: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Pr {
    pub pr_key: String, // "{owner}/{repo}#{number}"
    pub owner: String,
    pub repo: String,
    pub number: i64,
    pub author: String,
    pub title: String,
    pub url: String,

    pub updated_at_unix: i64,
    pub last_commit_sha: Option<String>,
    pub ci_state: CiState,
    pub ci_checks: Vec<CiCheck>,
    pub review_state: ReviewState,

    // Extra metadata for triage.
    pub is_draft: bool,
    pub mergeable: Option<String>, // e.g. "MERGEABLE" | "CONFLICTING" | "UNKNOWN"
    pub merge_state_status: Option<String>, // e.g. "CLEAN" | "BLOCKED" | ...
    pub is_viewer_author: bool,    // true when this PR is authored by the signed-in user
    pub merge_blockers: Option<MergeBlockers>,
}
