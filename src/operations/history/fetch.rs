//! Shallow checkout of one remote. This is the history operation that launches
//! `git`. `git_history` still reads objects in the active repository.

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_str, reject_unknown_arguments};
use blazingly_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

const SCHEMES: &[&str] = &["https://", "http://", "ssh://", "git://", "file://"];

pub fn fetch(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments("git_fetch", args, &["url", "ref"])?;
    let url = accept_url(arg_str(args, "url")?)?;
    let requested = optional_str(args, "ref")?.map(accept_ref).transpose()?;
    let refspec = requested.unwrap_or_else(|| "HEAD".to_owned());
    let identity = identity(&url)?;
    let cache = cache_root(state.root());
    fs::create_dir_all(&cache)
        .map_err(|error| format!("cannot create the fetch cache: {error}"))?;
    let id = fingerprint(&identity);
    let dest = cache.join(&id);
    let _lock = acquire_lock(&cache.join(format!("{id}.lock")))?;
    let reused = prepare(&dest, &url, &identity)?;
    git(
        "fetch",
        Some(&dest),
        &["fetch", "--depth", "1", "origin", &refspec],
    )?;
    git(
        "checkout",
        Some(&dest),
        &["checkout", "--force", "--detach", "FETCH_HEAD"],
    )?;
    let commit = git("rev-parse", Some(&dest), &["rev-parse", "HEAD"])?;
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("git rev-parse did not return a commit".to_owned());
    }
    Ok(json!({
        "status": "COMPLETE",
        "url": url,
        "ref": refspec,
        "commit": commit,
        "checkout": path_text(&plain_path(&dest)),
        "depth": 1,
        "reused": reused,
        "active_root": state.root(),
    }))
}

fn prepare(dest: &Path, url: &str, identity: &str) -> Result<bool, String> {
    if dest
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err("fetch cache path is a symlink".to_owned());
    }
    if dest.join(".git").is_dir() {
        if read_marker(dest)?.is_some_and(|found| found != identity) {
            return Err("fetch cache entry does not match this url".to_owned());
        }
        write_marker(dest, identity)?;
        ensure_origin(dest, url)?;
        return Ok(true);
    }
    if dest.exists() {
        remove_path(dest)?;
    }
    let dest_arg = path_text(&plain_path(dest));
    git("init", None, &["init", "-q", &dest_arg])?;
    ensure_origin(dest, url)?;
    write_marker(dest, identity)?;
    Ok(false)
}

fn ensure_origin(dest: &Path, url: &str) -> Result<(), String> {
    let listed = git("remote", Some(dest), &["remote"])?;
    if listed.split_whitespace().any(|name| name == "origin") {
        git("remote", Some(dest), &["remote", "set-url", "origin", url])?;
    } else {
        git("remote", Some(dest), &["remote", "add", "origin", url])?;
    }
    Ok(())
}

fn accept_url(raw: &str) -> Result<String, String> {
    let url = raw.trim();
    if url.is_empty() || url.len() > 2048 {
        return Err("url must be 1 to 2048 characters".to_owned());
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) || url.starts_with('-') {
        return Err("url must not contain whitespace or start with '-'".to_owned());
    }
    if let Some((scheme, rest)) = scheme_rest(url) {
        check_remote(scheme, rest)?;
        return Ok(url.to_owned());
    }
    if url.contains('@') {
        if !scp_style(url) {
            return Err("url must not embed credentials".to_owned());
        }
        return Ok(url.to_owned());
    }
    let path = Path::new(url);
    if !path.is_absolute() {
        return Err(
            "url must be an https, ssh, git, or file remote, or an absolute local path".to_owned(),
        );
    }
    if !path.exists() {
        return Err(format!("local url does not exist: {url}"));
    }
    Ok(url.to_owned())
}

fn accept_ref(raw: &str) -> Result<String, String> {
    let bad_edge = raw.is_empty()
        || raw.len() > 256
        || raw.starts_with('-')
        || raw.starts_with('/')
        || raw.ends_with('/')
        || raw.ends_with('.');
    let bad_seq =
        raw.contains("..") || raw.contains("//") || raw.contains("@{") || raw.contains(".lock");
    let bad_char = raw
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '/' | '_' | '.' | '-' | '+'));
    if bad_edge || bad_seq || bad_char {
        return Err("ref must be a branch, tag, or commit".to_owned());
    }
    Ok(raw.to_owned())
}

fn scheme_rest(url: &str) -> Option<(&str, &str)> {
    SCHEMES
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme).map(|rest| (*scheme, rest)))
}

fn check_remote(scheme: &str, rest: &str) -> Result<(), String> {
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() && !scheme.starts_with("file") {
        return Err("url is missing a host".to_owned());
    }
    let Some((info, host)) = authority.rsplit_once('@') else {
        return Ok(());
    };
    if host.is_empty() || info.is_empty() {
        return Err("url is missing a host".to_owned());
    }
    let embeds_secret = scheme.starts_with("https")
        || scheme.starts_with("http")
        || scheme.starts_with("file")
        || info.contains(':');
    if embeds_secret {
        return Err("url must not embed credentials".to_owned());
    }
    Ok(())
}

fn scp_style(url: &str) -> bool {
    let Some((user, rest)) = url.split_once('@') else {
        return false;
    };
    if user.is_empty() || user.contains(':') || rest.contains('@') {
        return false;
    }
    let Some((host, path)) = rest.split_once(':') else {
        return false;
    };
    !host.is_empty() && !host.contains('/') && !path.is_empty()
}

fn identity(url: &str) -> Result<String, String> {
    if scheme_rest(url).is_some() || scp_style(url) {
        return Ok(remote_identity(url));
    }
    let canonical = Path::new(url)
        .canonicalize()
        .map_err(|error| format!("cannot resolve local url: {error}"))?;
    let mut text = path_text(&plain_path(&canonical));
    if cfg!(windows) {
        text = text.to_ascii_lowercase();
    }
    Ok(text)
}

fn remote_identity(url: &str) -> String {
    let mut value = url.trim_end_matches('/').to_owned();
    if let Some(stripped) = value.strip_suffix(".git")
        && !stripped.is_empty()
        && !stripped.ends_with(':')
        && !stripped.ends_with('/')
    {
        value = stripped.to_owned();
    }
    value
}

fn cache_root(active: &Path) -> PathBuf {
    if let Some(home) = std::env::var_os("WEAVATRIX_FETCH_HOME") {
        return PathBuf::from(home);
    }
    if path_is_under_temp(active) {
        return std::env::temp_dir().join("weavatrix-fetches");
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map_or_else(std::env::temp_dir, PathBuf::from);
    home.join(".weavatrix").join("fetches")
}

fn path_is_under_temp(path: &Path) -> bool {
    canonical_plain(path).starts_with(canonical_plain(&std::env::temp_dir()))
}

fn canonical_plain(path: &Path) -> PathBuf {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let plain = plain_path(&resolved);
    if cfg!(windows) {
        PathBuf::from(plain.to_string_lossy().to_ascii_lowercase())
    } else {
        plain
    }
}

fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn fingerprint(identity: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"weavatrix-fetch-v1\0");
    hasher.update(identity.as_bytes());
    let mut encoded = String::with_capacity(32);
    for byte in hasher.finalize().iter().take(16) {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn marker_path(dest: &Path) -> PathBuf {
    dest.join(".weavatrix-fetch")
}

fn read_marker(dest: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(marker_path(dest)) {
        Ok(text) => Ok(Some(text.trim().to_owned())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read the fetch marker: {error}")),
    }
}

fn write_marker(dest: &Path, identity: &str) -> Result<(), String> {
    fs::write(marker_path(dest), identity)
        .map_err(|error| format!("cannot write the fetch marker: {error}"))
}

fn remove_path(path: &Path) -> Result<(), String> {
    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|error| format!("cannot replace the fetch cache entry: {error}"))
}

struct Lock(PathBuf);

impl Drop for Lock {
    fn drop(&mut self) {
        fs::remove_dir(&self.0).ok();
    }
}

fn acquire_lock(path: &Path) -> Result<Lock, String> {
    for attempt in 0..40 {
        match fs::create_dir(path) {
            Ok(()) => return Ok(Lock(path.to_owned())),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if attempt == 0 && lock_is_stale(path) {
                    fs::remove_dir(path).ok();
                    continue;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(format!("cannot lock the fetch cache: {error}")),
        }
    }
    Err("another git_fetch for this url is in progress".to_owned())
}

fn lock_is_stale(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > Duration::from_secs(600))
}

fn git(label: &str, cwd: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("safe.directory=*")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .map_err(|error| format!("git is not available: {error}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("git failed");
    let detail = detail.chars().take(240).collect::<String>();
    Err(format!("git {label} failed: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::{accept_ref, accept_url, identity};

    #[test]
    fn remote_urls_share_a_git_suffix_and_refuse_credentials() {
        let plain = accept_url("https://github.com/org/repo").unwrap();
        let suffixed = accept_url("https://github.com/org/repo.git/").unwrap();
        assert_eq!(identity(&plain).unwrap(), identity(&suffixed).unwrap());
        assert!(accept_url("https://token@github.com/org/repo").is_err());
        assert!(accept_url("https://user:token@github.com/org/repo").is_err());
        assert!(accept_url("ssh://git:secret@github.com/org/repo").is_err());
        assert!(accept_url("-https://github.com/org/repo").is_err());
        assert!(accept_url("ssh://git@github.com/org/repo.git").is_ok());
        assert!(accept_url("git@github.com:org/repo.git").is_ok());
        assert!(accept_ref("main").is_ok());
        assert!(accept_ref("refs/tags/v1.2.3").is_ok());
        assert!(accept_ref("--upload-pack=touch").is_err());
        assert!(accept_ref("main:refs/heads/main").is_err());
        assert!(accept_ref("HEAD~1").is_err());
    }
}
