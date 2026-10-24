//! `sinteractive claude hook scratch-guard` — a `PreToolUse` hook on `Bash`,
//! `Write` and `Edit` that denies the writes that filled the quota, and says
//! where the file should go instead.
//!
//! It denies, and nothing else: it never deletes, never prompts, never
//! reads. Silence means the normal permission flow decides.
//!
//! - **Bulk data into a worktree.** `cp`, `mv`, `rsync` or `install` of
//!   `.pod5 .fast5 .bam .cram .npz .npy .parquet .fastq .fq.gz .arrow` (or a
//!   `tar -x` naming one) to a path under `.claude/worktrees/`, or under the
//!   cluster's worktree scratch. A symlink (`ln -s`) is the sanctioned way
//!   to give a worktree its data and is never matched. The main checkout is
//!   not covered: its `data/` and `results/` trees are where canonical data
//!   lives.
//! - **New directories under `~/.cache`.** Anything is allowed inside a tool
//!   cache that already exists; creating a new one — a benchmark's output,
//!   a sweep — is not. Results go in the repo's `results/`, or in
//!   `~/scratch/<topic>/` (`sinteractive scratch new`).
//! - **A `Write` of 1 GiB or more into a worktree**, and **of a `.csv` of
//!   100 MiB or more anywhere** (`.csv.gz` or Parquet instead).
//!
//! The Bash command is split into simple commands on `;`, `&&`, `||`, `|`
//! and newlines, tracking `cd`. That is a heuristic and not a sandbox: it
//! catches the way agents actually copy data, not every way a shell can.
//! It does no I/O beyond an existence check on `~/.cache/<name>`, because it
//! runs before every Bash call.

use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

/// Extensions of bulk data; compared lower-case against a path's end.
const BULK: [&str; 11] = [
    ".pod5",
    ".fast5",
    ".bam",
    ".cram",
    ".npz",
    ".npy",
    ".parquet",
    ".fastq",
    ".fq.gz",
    ".fastq.gz",
    ".arrow",
];
const WORKTREE_WRITE_LIMIT: usize = 1 << 30;
const CSV_LIMIT: usize = 100 << 20;

pub fn run() -> Result<i32> {
    let input = super::hook::hook_input()?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if let Some(reason) = verdict(&input, &home) {
        println!(
            "{}",
            json!({"hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }})
        );
    }
    Ok(0)
}

fn is_bulk(path: &str) -> bool {
    let p = path.trim_end_matches('/').to_lowercase();
    BULK.iter().any(|e| p.ends_with(e))
}

/// `~`, `$HOME` and relative paths resolved, `.` and `..` folded lexically.
fn expand(word: &str, home: &Path, cwd: &Path) -> PathBuf {
    let rest = |prefix: &str| word.strip_prefix(prefix).map(|r| r.trim_start_matches('/'));
    let joined = if word == "~" || word == "$HOME" || word == "${HOME}" {
        home.to_path_buf()
    } else if let Some(r) = rest("~/").or(rest("$HOME/")).or(rest("${HOME}/")) {
        home.join(r)
    } else {
        cwd.join(word)
    };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    out
}

/// Under a `.claude/worktrees/` directory, or the worktree scratch the
/// `worktree-create` hook uses on Alpine.
fn in_worktree(path: &Path) -> bool {
    let names: Vec<_> = path.components().map(|c| c.as_os_str()).collect();
    names
        .windows(2)
        .any(|w| w[0] == ".claude" && w[1] == "worktrees")
        || (path.starts_with("/scratch/alpine") && names.iter().any(|n| *n == "worktrees"))
}

/// `~/.cache/<name>[/…]` where `<name>` does not exist yet: a new directory
/// in the tool-cache area.
fn new_cache_entry(path: &Path, home: &Path) -> bool {
    let cache = home.join(".cache");
    path.strip_prefix(&cache)
        .ok()
        .and_then(|r| r.components().next())
        .is_some_and(|first| !cache.join(first).exists())
}

fn bulk_into_worktree(what: &str, dest: &Path) -> String {
    format!(
        "Not copying {what} into a worktree ({}): worktrees hold code, and a copy outlives \
         its branch — 450 GiB of the quota was pod5 copies like this. Reference the \
         canonical path, or give the worktree a symlink (`ln -s CANONICAL {}`). If the \
         files are genuinely new working data, put them in `sinteractive scratch new \
         TOPIC` (the scratch_new tool).",
        dest.display(),
        dest.display()
    )
}

fn new_cache_dir(path: &Path, home: &Path) -> String {
    format!(
        "Not creating {} under ~/.cache: that is where tool caches go and where a cleanup \
         looks first, so results parked there are one `rm -rf` from gone. Benchmark and \
         probe output goes in the repo's `results/`, or in `sinteractive scratch new TOPIC` \
         (under {}/scratch/) if it is throwaway.",
        path.display(),
        home.display()
    )
}

/// The reason to deny this tool call, if it should be denied.
pub fn verdict(input: &Value, home: &Path) -> Option<String> {
    let tool = input.get("tool_name").and_then(Value::as_str)?;
    let ti = input.get("tool_input")?;
    let cwd = input
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    match tool {
        "Write" | "Edit" => {
            let path = expand(ti.get("file_path")?.as_str()?, home, &cwd);
            let len = ["content", "new_string"]
                .iter()
                .filter_map(|k| ti.get(k).and_then(Value::as_str))
                .map(str::len)
                .max()
                .unwrap_or(0);
            file_verdict(&path, len, home)
        }
        "Bash" => bash_verdict(ti.get("command")?.as_str()?, home, &cwd),
        _ => None,
    }
}

/// A `Write` or `Edit` of `len` bytes to `path`.
fn file_verdict(path: &Path, len: usize, home: &Path) -> Option<String> {
    let name = path.to_string_lossy().to_lowercase();
    if new_cache_entry(path, home) {
        Some(new_cache_dir(path, home))
    } else if in_worktree(path) && is_bulk(&name) {
        Some(bulk_into_worktree("bulk data", path))
    } else if in_worktree(path) && len >= WORKTREE_WRITE_LIMIT {
        Some(format!(
            "Not writing {} MiB into a worktree: results and corpora belong in the repo's \
             results/ or data/ tree, or `sinteractive scratch new TOPIC`.",
            len >> 20
        ))
    } else if name.ends_with(".csv") && len >= CSV_LIMIT {
        Some(format!(
            "Not writing a {} MiB .csv: write .csv.gz or Parquet (pandas and polars read \
             and write either).",
            len >> 20
        ))
    } else {
        None
    }
}

/// One simple command: its words, and the targets of its `>` / `>>`.
#[derive(Default)]
struct Simple {
    words: Vec<String>,
    redirects: Vec<String>,
}

/// Split a shell command line into simple commands. Quotes and backslashes
/// group words; `;`, `&&`, `||`, `|`, `&` and newlines end a command; `>`
/// and `>>` make the next word a redirect target. Anything cleverer — here
/// documents, `$(…)`, `eval` — is not followed.
#[allow(unused_assignments)] // the last flush! resets state nobody reads
fn split(command: &str) -> Vec<Simple> {
    let mut out = vec![Simple::default()];
    let mut word = String::new();
    let mut have_word = false;
    let mut redirect = false;
    let mut chars = command.chars().peekable();
    macro_rules! flush {
        () => {
            if have_word {
                let w = std::mem::take(&mut word);
                let cur = out.last_mut().unwrap();
                if std::mem::take(&mut redirect) {
                    cur.redirects.push(w);
                } else {
                    cur.words.push(w);
                }
                have_word = false;
            }
        };
    }
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                have_word = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    word.push(q);
                }
            }
            '"' => {
                have_word = true;
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' => word.extend(chars.next()),
                        q => word.push(q),
                    }
                }
            }
            '\\' => {
                have_word = true;
                word.extend(chars.next());
            }
            ' ' | '\t' => flush!(),
            '>' => {
                // `2>` names a file descriptor, not an argument.
                if have_word && word.chars().all(|c| c.is_ascii_digit()) {
                    word.clear();
                    have_word = false;
                }
                flush!();
                if chars.peek() == Some(&'>') {
                    chars.next();
                }
                // `2>&1` duplicates a descriptor; there is no file.
                if chars.peek() == Some(&'&') {
                    chars.next();
                    while chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-')
                    {
                        chars.next();
                    }
                } else {
                    redirect = true;
                }
            }
            ';' | '|' | '&' | '\n' => {
                flush!();
                redirect = false;
                if matches!(c, '|' | '&') && chars.peek() == Some(&c) {
                    chars.next();
                }
                if !out.last().unwrap().words.is_empty()
                    || !out.last().unwrap().redirects.is_empty()
                {
                    out.push(Simple::default());
                }
            }
            c => {
                have_word = true;
                word.push(c);
            }
        }
    }
    flush!();
    out
}

fn is_assignment(w: &str) -> bool {
    w.split_once('=').is_some_and(|(k, _)| {
        !k.is_empty()
            && !k.starts_with(|c: char| c.is_ascii_digit())
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn bash_verdict(command: &str, home: &Path, start: &Path) -> Option<String> {
    let mut cwd = start.to_path_buf();
    for simple in split(command) {
        // Drop leading assignments and the wrappers that run the rest.
        let mut words: &[String] = &simple.words;
        while let Some(first) = words.first() {
            let base = first.rsplit('/').next().unwrap_or(first);
            if is_assignment(first) || matches!(base, "sudo" | "time" | "nohup" | "env" | "command")
            {
                words = &words[1..];
            } else {
                break;
            }
        }
        let Some(cmd) = words.first().map(|w| w.rsplit('/').next().unwrap_or(w)) else {
            // Only a redirect: `> file`.
            if let Some(r) = redirect_verdict(&simple.redirects, home, &cwd) {
                return Some(r);
            }
            continue;
        };
        let args = &words[1..];
        let mut flags_done = false;
        let mut operands: Vec<&str> = Vec::new();
        let mut target_dir: Option<&str> = None;
        let mut it = args.iter().map(String::as_str);
        while let Some(a) = it.next() {
            if flags_done || !a.starts_with('-') || a == "-" {
                operands.push(a);
            } else if a == "--" {
                flags_done = true;
            } else if a == "-t" || a == "--target-directory" {
                target_dir = it.next();
            } else if let Some(d) = a.strip_prefix("--target-directory=") {
                target_dir = Some(d);
            }
        }
        match cmd {
            "cd" => {
                if let Some(d) = operands.first() {
                    cwd = expand(d, home, &cwd);
                }
            }
            "cp" | "mv" | "rsync" | "install" => {
                let (sources, dest) = match target_dir {
                    Some(d) => (operands.as_slice(), Some(d)),
                    None => match operands.split_last() {
                        Some((d, s)) if !s.is_empty() => (s, Some(*d)),
                        _ => (&[][..], None),
                    },
                };
                if let Some(d) = dest {
                    let dest = expand(d, home, &cwd);
                    if new_cache_entry(&dest, home) {
                        return Some(new_cache_dir(&dest, home));
                    }
                    if in_worktree(&dest) {
                        if let Some(s) = sources
                            .iter()
                            .find(|s| is_bulk(s))
                            .or(is_bulk(d).then_some(&d))
                        {
                            return Some(bulk_into_worktree(&format!("`{s}`"), &dest));
                        }
                    }
                }
            }
            "tar" | "bsdtar" => {
                let extracting = args.iter().enumerate().any(|(i, a)| {
                    a == "--extract"
                        || a == "--get"
                        || (a.starts_with('-') && !a.starts_with("--") && a.contains('x'))
                        || (i == 0 && !a.starts_with('-') && a.contains('x'))
                });
                let dir = args
                    .iter()
                    .position(|a| a == "-C" || a == "--directory")
                    .and_then(|i| args.get(i + 1))
                    .map(|d| expand(d, home, &cwd))
                    .unwrap_or_else(|| cwd.clone());
                if extracting && in_worktree(&dir) {
                    if let Some(s) = args.iter().find(|a| is_bulk(a) || a.contains(".pod5")) {
                        return Some(bulk_into_worktree(&format!("`{s}`"), &dir));
                    }
                }
            }
            "mkdir" | "touch" | "tee" => {
                for o in &operands {
                    let p = expand(o, home, &cwd);
                    if new_cache_entry(&p, home) {
                        return Some(new_cache_dir(&p, home));
                    }
                }
            }
            _ => {}
        }
        if let Some(r) = redirect_verdict(&simple.redirects, home, &cwd) {
            return Some(r);
        }
    }
    None
}

fn redirect_verdict(targets: &[String], home: &Path, cwd: &Path) -> Option<String> {
    for t in targets {
        if t.starts_with('&') || t == "/dev/null" {
            continue;
        }
        let p = expand(t, home, cwd);
        if new_cache_entry(&p, home) {
            return Some(new_cache_dir(&p, home));
        }
        if in_worktree(&p) && is_bulk(t) {
            return Some(bulk_into_worktree("bulk data", &p));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const WT: &str = "/h/devel/proj/.claude/worktrees/feat";

    fn bash(command: &str, cwd: &str, home: &Path) -> Option<String> {
        verdict(
            &json!({"tool_name": "Bash", "cwd": cwd, "tool_input": {"command": command}}),
            home,
        )
    }

    fn home() -> tempfile::TempDir {
        let h = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(h.path().join(".cache/pip")).unwrap();
        h
    }

    #[test]
    fn denies_cp_of_pod5_into_a_worktree() {
        let h = home();
        for cmd in [
            format!("cp /data/run/PBK76980_0.pod5 {WT}/data/"),
            format!("cp -r /data/run/*.pod5 {WT}/"),
            format!("mv /data/a.bam {WT}/a.bam"),
            format!("rsync -av -e ssh /data/a.npy {WT}/"),
            format!("cp -t {WT}/data /data/a.parquet"),
            "cp /data/a.fq.gz .".to_string(),
            "cp /data/a.pod5 data/a.pod5".to_string(),
            format!("cd {WT} && cp /data/a.pod5 ."),
            format!("echo hi; cp /data/a.pod5 '{WT}/x y/'"),
        ] {
            let got = bash(&cmd, WT, h.path());
            assert!(
                got.as_deref().is_some_and(|r| r.contains("scratch new")),
                "{cmd}: {got:?}"
            );
        }
        let tar = bash(
            &format!("tar -xf runs.tar -C {WT} --wildcards '*.pod5'"),
            "/h",
            h.path(),
        );
        assert!(tar.is_some(), "tar -x naming pod5");
        let redirect = bash(&format!("cat a > {WT}/a.npy"), "/h", h.path());
        assert!(redirect.is_some(), "redirect into a bulk file");
        // A trailing `2>&1` is not an operand: the destination is still found.
        let fd = bash(&format!("cp /data/a.pod5 {WT}/ 2>&1"), "/h", h.path());
        assert!(fd.is_some(), "cp with 2>&1");
    }

    #[test]
    fn allows_symlinks_and_everything_that_is_not_bulk_into_a_worktree() {
        let h = home();
        for cmd in [
            format!("ln -s /data/run/a.pod5 {WT}/data/a.pod5"),
            format!("ln -sf /data/run {WT}/data"),
            "cp /data/a.pod5 /h/scratch/topic/".to_string(), // not a worktree
            format!("cp src/lib.rs {WT}/src/"),
            format!("cp /data/run/ {WT}/data"), // a directory: not judged
            format!("cp -r {WT}/a.pod5 /h/results/"), // out of the worktree
            "ls *.pod5".to_string(),
            format!("echo 'a.pod5' > {WT}/notes.txt"),
            format!("tar -xf code.tar -C {WT}"),
            format!("tar -cf out.tar {WT}/a.pod5"), // creating, not extracting
        ] {
            assert_eq!(bash(&cmd, "/h/other", h.path()), None, "{cmd}");
        }
    }

    #[test]
    fn denies_cache_dir_results() {
        let h = home();
        let hp = h.path().display();
        for cmd in [
            format!("mkdir -p {hp}/.cache/ldx-crf-sweep"),
            "mkdir ~/.cache/ecoli_scan".to_string(),
            "cp out.csv $HOME/.cache/simbench/".to_string(),
            format!("python run.py > {hp}/.cache/lcurve/log.txt"),
            "echo x >> ~/.cache/squiggy/out".to_string(),
            "cd ~/.cache && mkdir refseq_panel".to_string(),
            "python run.py 2>&1 | tee ~/.cache/rfam_models/log".to_string(),
        ] {
            let got = bash(&cmd, "/h", h.path());
            // `~` resolves through the real $HOME in the live hook; here the
            // test home is injected, so only the explicit-path cases and the
            // `$HOME`/`~` forms (expanded with the injected home) are judged.
            assert!(
                got.as_deref().is_some_and(|r| r.contains("~/.cache")),
                "{cmd}: {got:?}"
            );
        }
        for (path, tool) in [
            ("/.cache/ldx16_full/res.csv.gz", "Write"),
            ("/.cache/ldx16_full/a", "Edit"),
        ] {
            let v = verdict(
                &json!({"tool_name": tool, "tool_input": {"file_path": format!("{hp}{path}"), "content": "x"}}),
                h.path(),
            );
            assert!(v.is_some(), "{tool} {path}");
        }
    }

    #[test]
    fn allows_normal_cache_paths() {
        let h = home();
        let hp = h.path().display();
        for cmd in [
            format!("pip install x --cache-dir {hp}/.cache/pip"),
            format!("cp wheel.whl {hp}/.cache/pip/wheels/"),
            format!("echo x > {hp}/.cache/pip/log"),
            format!("mkdir -p {hp}/.cache/pip/http"),
            format!("ls {hp}/.cache"),
            format!("cat {hp}/.cache/never-created/x"),
            format!("cp a b {hp}/scratch/"),
        ] {
            assert_eq!(bash(&cmd, "/h", h.path()), None, "{cmd}");
        }
        let v = verdict(
            &json!({"tool_name": "Write", "tool_input": {"file_path": format!("{hp}/.cache/pip/x"), "content": "x"}}),
            h.path(),
        );
        assert_eq!(v, None);
    }

    #[test]
    fn write_and_edit_are_judged_by_path_and_size() {
        let h = home();
        let write = |path: &str, len: usize| file_verdict(Path::new(path), len, h.path());
        assert!(write(&format!("{WT}/data/a.npy"), 10).is_some());
        assert!(write(&format!("{WT}/notes.md"), 10).is_none());
        assert!(write(&format!("{WT}/big.txt"), WORKTREE_WRITE_LIMIT).is_some());
        assert!(write("/h/results/big.txt", WORKTREE_WRITE_LIMIT).is_none());
        assert!(write("/h/results/fp.csv", CSV_LIMIT)
            .unwrap()
            .contains(".csv.gz"));
        assert!(write("/h/results/fp.csv", CSV_LIMIT - 1).is_none());
        assert!(write("/h/results/fp.csv.gz", CSV_LIMIT).is_none());
        // Through the hook input, with a small body.
        let v = verdict(
            &json!({"tool_name": "Edit", "cwd": WT, "tool_input": {
                "file_path": "data/x.pod5", "new_string": "x"}}),
            h.path(),
        );
        assert!(v.is_some());
        // Other tools and malformed input are none of its business.
        assert_eq!(
            verdict(&json!({"tool_name": "Read", "tool_input": {}}), h.path()),
            None
        );
        assert_eq!(verdict(&json!({}), h.path()), None);
    }

    #[test]
    fn the_splitter_follows_quotes_separators_and_redirects() {
        let s = split(r#"A=1 cp "a b.pod5" 'c d' dest/ && echo hi > out.txt; ls | wc >> log"#);
        assert_eq!(s.len(), 4);
        assert_eq!(s[0].words, ["A=1", "cp", "a b.pod5", "c d", "dest/"]);
        assert_eq!(s[1].words, ["echo", "hi"]);
        assert_eq!(s[1].redirects, ["out.txt"]);
        assert_eq!(s[3].redirects, ["log"]);
        assert_eq!(split("echo a>b")[0].redirects, ["b"]);
        let fd = &split("cp a b 2>&1 | cat")[0];
        assert_eq!(fd.words, ["cp", "a", "b"]);
        assert!(fd.redirects.is_empty());
        assert_eq!(split("cmd 2>err.log")[0].redirects, ["err.log"]);
        assert!(split("").iter().all(|c| c.words.is_empty()));
    }

    #[test]
    fn paths_expand_and_fold() {
        let home = Path::new("/h");
        let cwd = Path::new("/h/p");
        assert_eq!(expand("~/x", home, cwd), PathBuf::from("/h/x"));
        assert_eq!(expand("$HOME/x", home, cwd), PathBuf::from("/h/x"));
        assert_eq!(expand("a/../b/./c", home, cwd), PathBuf::from("/h/p/b/c"));
        assert_eq!(expand("../q", home, cwd), PathBuf::from("/h/q"));
        assert_eq!(expand("/abs", home, cwd), PathBuf::from("/abs"));
        assert!(in_worktree(Path::new("/x/.claude/worktrees/w/a")));
        assert!(in_worktree(Path::new("/scratch/alpine/u/worktrees/r/w")));
        assert!(!in_worktree(Path::new("/x/worktrees/w")));
        assert!(!in_worktree(Path::new("/x/.claude/settings")));
    }
}
