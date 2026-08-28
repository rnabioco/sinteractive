//! `sbatch` submission.
//!
//! Ports script lines 634-683: submit with `--output=/dev/null
//! --error=/dev/null`, capture stdout and stderr separately, scrape the job id
//! from the trailing integer of stdout, echo the passthrough args as a hint on
//! failure.

use super::{Slurm, SlurmError};

/// What a successful submission produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    pub job_id: u64,
    /// sbatch's stderr on success: warnings worth showing the user
    /// (the bash surfaced these verbatim). Empty when it said nothing.
    pub warnings: String,
}

/// Extract the job id from `sbatch` stdout ("Submitted batch job 12345" or
/// `--parsable` "12345" / "12345;cluster").
pub fn parse_job_id(stdout: &str) -> Option<u64> {
    let line = stdout.lines().rev().find(|l| !l.trim().is_empty())?.trim();
    // --parsable with a cluster name: "12345;cluster".
    if let Some((head, _)) = line.split_once(';') {
        if let Ok(id) = head.trim().parse() {
            return Some(id);
        }
    }
    // Trailing integer at a word boundary, as `grep -Eo '\b[0-9]+$'`.
    let digits = line
        .bytes()
        .rev()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let start = line.len() - digits;
    let before = line[..start].chars().next_back();
    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    line[start..].parse().ok()
}

impl Slurm {
    /// Submit `script` with `args` (sbatch options first, then the script and
    /// its arguments). Returns the job id.
    pub fn sbatch(&self, args: &[String]) -> Result<u64, SlurmError> {
        self.submit(args).map(|s| s.job_id)
    }

    /// Like [`Slurm::sbatch`] but also hands back sbatch's warnings, which a
    /// CLI should print even on success.
    pub fn submit(&self, args: &[String]) -> Result<Submission, SlurmError> {
        let mut cmd = self.command("sbatch");
        cmd.arg("--output=/dev/null")
            .arg("--error=/dev/null")
            .args(args);
        let out = Slurm::output_retrying(&mut cmd, "sbatch")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !out.status.success() {
            return Err(SlurmError::Failed {
                cmd: "sbatch".into(),
                status: out.status.code().unwrap_or(-1),
                stderr: if stderr.is_empty() {
                    "(no output)".into()
                } else {
                    stderr
                },
            });
        }
        let job_id = parse_job_id(&stdout).ok_or_else(|| SlurmError::Parse {
            cmd: "sbatch".into(),
            reason: format!(
                "sbatch succeeded but its output contained no job id: {}",
                if stdout.trim().is_empty() {
                    "(no output)"
                } else {
                    stdout.trim()
                }
            ),
        })?;
        Ok(Submission {
            job_id,
            warnings: stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_output() {
        assert_eq!(parse_job_id("Submitted batch job 12345\n"), Some(12345));
        assert_eq!(parse_job_id("Submitted batch job 12345"), Some(12345));
    }

    #[test]
    fn parses_parsable_output() {
        assert_eq!(parse_job_id("12345\n"), Some(12345));
        assert_eq!(parse_job_id("12345;alpine\n"), Some(12345));
    }

    #[test]
    fn uses_last_non_empty_line() {
        assert_eq!(
            parse_job_id("sbatch: some banner\nSubmitted batch job 99\n\n"),
            Some(99)
        );
    }

    #[test]
    fn rejects_output_without_trailing_integer() {
        assert_eq!(parse_job_id(""), None);
        assert_eq!(parse_job_id("Submitted batch job\n"), None);
        assert_eq!(parse_job_id("job abc123"), None);
        assert_eq!(parse_job_id("Submitted batch job 12345 on cluster"), None);
    }

    #[test]
    fn missing_binary_is_not_found() {
        let s = Slurm {
            bin_dir: Some(std::path::PathBuf::from("/nonexistent/sint-test-bin")),
        };
        assert!(matches!(
            s.sbatch(&["script.sh".to_string()]),
            Err(SlurmError::NotFound { .. })
        ));
    }

    #[cfg(unix)]
    mod with_fake_sbatch {
        use super::*;
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        fn fake(script: &str) -> (tempfile::TempDir, Slurm) {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("sbatch");
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(script.as_bytes()).unwrap();
            drop(f);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            let s = Slurm {
                bin_dir: Some(dir.path().to_path_buf()),
            };
            (dir, s)
        }

        #[test]
        fn prepends_dev_null_output_and_returns_id() {
            let (_d, s) = fake(
                "#!/bin/sh\n\
                 printf '%s\\n' \"$@\" > \"$0.args\"\n\
                 echo 'sbatch: warning: something' >&2\n\
                 echo 'Submitted batch job 4242'\n",
            );
            let args = vec!["-p".to_string(), "rna".to_string(), "run.sh".to_string()];
            let sub = s.submit(&args).unwrap();
            assert_eq!(sub.job_id, 4242);
            assert_eq!(sub.warnings, "sbatch: warning: something");
            let seen = std::fs::read_to_string(_d.path().join("sbatch.args")).unwrap();
            assert_eq!(
                seen,
                "--output=/dev/null\n--error=/dev/null\n-p\nrna\nrun.sh\n"
            );
            assert_eq!(s.sbatch(&args).unwrap(), 4242);
        }

        #[test]
        fn failure_carries_stderr() {
            let (_d, s) = fake(
                "#!/bin/sh\n\
                 echo 'sbatch: error: Batch job submission failed: Invalid qos' >&2\n\
                 exit 1\n",
            );
            match s.sbatch(&["x.sh".to_string()]) {
                Err(SlurmError::Failed {
                    cmd,
                    status,
                    stderr,
                }) => {
                    assert_eq!(cmd, "sbatch");
                    assert_eq!(status, 1);
                    assert!(stderr.contains("Invalid qos"), "{stderr}");
                }
                other => panic!("unexpected {other:?}"),
            }
        }

        #[test]
        fn success_without_id_is_parse_error() {
            let (_d, s) = fake("#!/bin/sh\necho 'nothing useful'\n");
            assert!(matches!(
                s.sbatch(&["x.sh".to_string()]),
                Err(SlurmError::Parse { .. })
            ));
        }
    }
}
