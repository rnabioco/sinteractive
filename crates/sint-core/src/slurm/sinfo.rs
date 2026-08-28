//! `sinfo` — node lists for `doctor --nodes`.

use super::{Slurm, SlurmError};

/// `sort -u` over the lines of `sinfo -hN -o %N`: trimmed, non-empty,
/// sorted, deduplicated (a node in several partitions is listed once).
pub fn parse_node_names(output: &str) -> Vec<String> {
    let mut names: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
}

impl Slurm {
    /// `sinfo -hN -o %N | sort -u`.
    pub fn node_names(&self) -> Result<Vec<String>, SlurmError> {
        let out = self.run("sinfo", &["-hN", "-o", "%N"])?;
        Ok(parse_node_names(&out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_and_dedups() {
        let out = "compute20\ncompute03\ncompute20\n\ncompute03 \nc3gpu-a2-u1-1\n";
        assert_eq!(
            parse_node_names(out),
            vec!["c3gpu-a2-u1-1", "compute03", "compute20"]
        );
        assert!(parse_node_names("").is_empty());
    }

    #[test]
    fn missing_binary() {
        let s = Slurm {
            bin_dir: Some(std::path::PathBuf::from("/nonexistent/sint-test-bin")),
        };
        assert!(matches!(s.node_names(), Err(SlurmError::NotFound { .. })));
    }
}
