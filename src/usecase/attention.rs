use crate::repo::github::model::{Pr, ReviewState};

/// Decide whether a PR should be added as a todo.
/// Current rule: add when the viewer is explicitly requested as a reviewer.
pub fn should_add_todo(pr: &Pr) -> bool {
    matches!(pr.review_state, ReviewState::Requested)
}
