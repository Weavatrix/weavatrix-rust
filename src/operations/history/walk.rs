use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use weavatrix_git::{Commit, GitError, HistoryOptions, HistoryRecord, ObjectId, Repository};

pub(super) struct Walked {
    pub records: Vec<HistoryRecord>,
    pub boundary: bool,
}

#[derive(Clone, Eq, PartialEq)]
struct Pending {
    time: i64,
    sequence: u64,
    commit: Commit,
}

impl Ord for Pending {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.time, self.sequence).cmp(&(other.time, other.sequence))
    }
}

impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Newest-first history. A missing parent is the shallow boundary. A missing
/// start commit stays an error. Parents of a commit older than `since` are
/// left unread, because that ancestor is outside the requested window.
pub(super) fn commits(
    repository: &Repository,
    start: ObjectId,
    options: HistoryOptions,
) -> Result<Walked, String> {
    let cap = capped(repository, options.max_commits)?;
    let first = repository
        .commit(start)
        .map_err(|error| error.to_string())?;
    let mut pending = BinaryHeap::from([Pending {
        time: commit_time(&first),
        sequence: 0,
        commit: first,
    }]);
    let mut scheduled = HashSet::from([start]);
    let mut records = Vec::new();
    let mut sequence = 1_u64;
    let mut boundary = false;
    while let Some(item) = pending.pop() {
        if records.len() == cap || options.since.is_some_and(|time| item.time < time) {
            break;
        }
        boundary |= enqueue(
            repository,
            &item.commit,
            options.first_parent,
            &mut pending,
            &mut scheduled,
            &mut sequence,
        )?;
        if options.until.is_none_or(|time| item.time <= time) {
            records.push(HistoryRecord {
                id: item.commit.id,
                commit: item.commit,
            });
        }
    }
    Ok(Walked { records, boundary })
}

fn capped(repository: &Repository, max_commits: usize) -> Result<usize, String> {
    let limit = repository.limits().max_history_commits;
    if max_commits > limit {
        return Err(GitError::LimitExceeded {
            resource: "history commits",
            limit,
        }
        .to_string());
    }
    Ok(max_commits)
}

fn enqueue(
    repository: &Repository,
    commit: &Commit,
    first_parent: bool,
    pending: &mut BinaryHeap<Pending>,
    scheduled: &mut HashSet<ObjectId>,
    sequence: &mut u64,
) -> Result<bool, String> {
    let parents = if first_parent {
        commit.parents.get(..1).unwrap_or(&[])
    } else {
        commit.parents.as_slice()
    };
    let mut boundary = false;
    for parent in parents {
        if !scheduled.insert(*parent) {
            continue;
        }
        match repository.commit(*parent) {
            Ok(parent_commit) => {
                pending.push(Pending {
                    time: commit_time(&parent_commit),
                    sequence: *sequence,
                    commit: parent_commit,
                });
                *sequence += 1;
            }
            Err(GitError::NotFound(_)) => boundary = true,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(boundary)
}

fn commit_time(commit: &Commit) -> i64 {
    commit
        .committer
        .as_ref()
        .or(commit.author.as_ref())
        .map_or(0, |signature| signature.timestamp)
}
