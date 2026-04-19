//! Critical path computation for a build's dependency graph.
//!
//! The critical path is the longest path through the build's dependency DAG,
//! representing the chain of compilations that determines the total build time.
//!
//! # Algorithm
//!
//! 1. Construct a DAG from crate compilation records (edges represent
//!    "A must finish before B can start" relationships).
//! 2. Perform a topological sort of the DAG.
//! 3. Use dynamic programming to find the longest path:
//!    - For each node in topological order, `dist[v] = max(dist[u] + weight(v))` for all predecessors u.
//! 4. The path ending at the node with maximum `dist` value is the critical path.
//!
//! # Note
//!
//! This requires dependency information that may need to come from `cargo metadata`
//! in addition to the build event stream. For MVP, we may approximate using
//! compilation start/finish times to infer ordering.

use crate::model::CrateCompilation;

/// Compute the critical path from a set of crate compilations.
///
/// Returns the ordered list of crate names forming the longest path
/// through the build dependency graph.
///
/// # Arguments
///
/// * `compilations` — All crate compilations from a single build.
///
/// # Returns
///
/// An ordered `Vec<String>` of crate names on the critical path,
/// from the first to the last in the chain.
pub fn compute_critical_path(_compilations: &[CrateCompilation]) -> Vec<String> {
    todo!(
        "Implement DAG longest path: \
         1) Build adjacency list from compilation overlap/ordering, \
         2) Topological sort, \
         3) DP for longest path, \
         4) Backtrack to recover path"
    )
}

#[cfg(test)]
mod tests {
    //! Contract tests for `compute_critical_path`.

    use super::*;
    use crate::model::{BuildId, CrateId};
    use std::time::Duration;

    fn compilation(name: &str, ms: u64) -> CrateCompilation {
        CrateCompilation {
            build_id: BuildId(1),
            crate_id: CrateId {
                name: name.to_string(),
                version: None,
            },
            kind: "lib".to_string(),
            started_at: "2025-01-01T00:00:00Z".to_string(),
            finished_at: "2025-01-01T00:00:01Z".to_string(),
            duration: Duration::from_millis(ms),
        }
    }

    #[test]
    fn empty_input_returns_empty_path() {
        assert!(compute_critical_path(&[]).is_empty());
    }

    #[test]
    fn single_crate_is_its_own_critical_path() {
        let path = compute_critical_path(&[compilation("solo", 100)]);
        assert_eq!(path, vec!["solo".to_string()]);
    }

    #[test]
    fn critical_path_includes_longest_crate() {
        // Loose contract: the slowest crate must appear on the critical path.
        let comps = vec![
            compilation("fast", 10),
            compilation("slow", 1000),
            compilation("medium", 100),
        ];
        let path = compute_critical_path(&comps);
        assert!(
            path.iter().any(|n| n == "slow"),
            "critical path should include the slowest crate; got {:?}",
            path
        );
    }
}
