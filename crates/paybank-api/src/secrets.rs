//! File-backed secret loading for Docker / Compose / Swarm secrets (and any
//! other secrets manager that delivers values as files, e.g. Kubernetes secret
//! volumes mounted under `/run/secrets`).
//!
//! Convention — the same one the official Postgres/MySQL images use: for any
//! environment variable `FOO`, if `FOO_FILE` is set and points to a readable
//! file, that file's contents become the value of `FOO`. This keeps the raw
//! secret out of the process environment (so it can't leak via `docker inspect`,
//! `/proc/<pid>/environ`, or a crash dump of the env) and off disk in a
//! plaintext `.env`.
//!
//! In Docker Compose you wire it up like:
//! ```yaml
//! services:
//!   api:
//!     environment:
//!       PII_ENCRYPTION_KEY_FILE: /run/secrets/pii_key
//!       DATABASE_URL_FILE:       /run/secrets/database_url
//!     secrets: [pii_key, database_url]
//! secrets:
//!   pii_key:      { file: ./secrets/pii_key }
//!   database_url: { file: ./secrets/database_url }
//! ```
//!
//! This MUST run before any secret consumer reads its variable — the crypto
//! `OnceLock`, `Config::from_env`, and the DB pool all read straight from the
//! environment. Call [`load_file_backed_secrets`] first thing in `main`.

use std::env;

/// Hydrate `FOO` from `FOO_FILE` for every `*_FILE` variable in the environment.
///
/// Returns the list of base variable names that were populated, so the caller
/// can emit a single startup log line naming *which* secrets came from files —
/// the values themselves are never logged.
///
/// Fails fast (rather than falling back to an unset or stale secret) when a
/// `*_FILE` variable points at a path that can't be read or is empty: a missing
/// secret at boot should stop the process, not silently degrade.
pub fn load_file_backed_secrets() -> anyhow::Result<Vec<String>> {
    // Snapshot the matching vars before mutating the environment underneath the
    // iterator.
    let file_vars: Vec<(String, String)> = env::vars()
        .filter(|(k, _)| k.len() > "_FILE".len() && k.ends_with("_FILE"))
        .collect();

    let mut loaded = Vec::new();
    for (file_key, path) in file_vars {
        let base = file_key
            .strip_suffix("_FILE")
            .expect("filtered to *_FILE keys");
        if path.trim().is_empty() {
            continue;
        }

        let raw = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("{file_key}={path}: cannot read secret file: {e}"))?;
        // Secret files almost always carry a trailing newline; strip only CR/LF
        // so any intentional inner character is preserved.
        let value = raw.trim_end_matches(['\n', '\r']).to_string();
        if value.is_empty() {
            anyhow::bail!("{file_key}={path}: secret file is empty");
        }

        // If the plain variable was also set directly, the file is the
        // secret-managed source of truth — prefer it, but warn on a real clash.
        if let Ok(existing) = env::var(base) {
            if !existing.is_empty() && existing != value {
                tracing::warn!(
                    var = base,
                    "both {base} and {file_key} are set; using the file-backed value"
                );
            }
        }

        env::set_var(base, value);
        loaded.push(base.to_string());
    }

    loaded.sort();
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // The environment is process-global, so all assertions live in one test fn
    // with uniquely-named vars to avoid cross-test interference.
    #[test]
    fn loads_trims_prefers_file_and_rejects_bad_paths() {
        let dir = std::env::temp_dir();

        // --- happy path: FOO_FILE with a trailing newline populates FOO ---
        let p = dir.join("noor_secret_happy");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "s3cr3t-value").unwrap();
        env::set_var("NOOR_TEST_HAPPY_FILE", p.to_str().unwrap());
        env::remove_var("NOOR_TEST_HAPPY");

        let loaded = load_file_backed_secrets().unwrap();
        assert!(loaded.contains(&"NOOR_TEST_HAPPY".to_string()));
        assert_eq!(env::var("NOOR_TEST_HAPPY").unwrap(), "s3cr3t-value");

        // --- file value wins over a directly-set base var ---
        env::set_var("NOOR_TEST_HAPPY", "stale-direct-value");
        load_file_backed_secrets().unwrap();
        assert_eq!(env::var("NOOR_TEST_HAPPY").unwrap(), "s3cr3t-value");

        // cleanup of the happy var so it can't leak into other cases
        env::remove_var("NOOR_TEST_HAPPY_FILE");
        env::remove_var("NOOR_TEST_HAPPY");
        std::fs::remove_file(&p).ok();

        // --- unreadable path is fatal (fail fast, no silent fallback) ---
        env::set_var(
            "NOOR_TEST_MISSING_FILE",
            dir.join("noor_does_not_exist_xyz").to_str().unwrap(),
        );
        assert!(load_file_backed_secrets().is_err());
        env::remove_var("NOOR_TEST_MISSING_FILE");

        // --- empty secret file is fatal ---
        let pe = dir.join("noor_secret_empty");
        std::fs::File::create(&pe).unwrap();
        env::set_var("NOOR_TEST_EMPTY_FILE", pe.to_str().unwrap());
        assert!(load_file_backed_secrets().is_err());
        env::remove_var("NOOR_TEST_EMPTY_FILE");
        std::fs::remove_file(&pe).ok();
    }
}
