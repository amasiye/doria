use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::backend::BackendError;

pub fn locate() -> Result<PathBuf, BackendError> {
    let current_executable = env::current_exe().map_err(|error| {
        BackendError::new(format!(
            "doria-rt static library was not found: failed to locate doriac: {error}\nhelp: build it with `cargo build -p doria-rt` or set DORIA_RT_PATH"
        ))
    })?;
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("doriac must live under the workspace crates directory");
    let target_override = env::var_os("CARGO_TARGET_DIR");
    resolve(
        env::var_os("DORIA_RT_PATH").as_deref(),
        &current_executable,
        workspace,
        target_override.as_deref(),
        cfg!(windows),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    )
}

fn resolve(
    explicit: Option<&OsStr>,
    current_executable: &Path,
    workspace: &Path,
    target_override: Option<&OsStr>,
    windows: bool,
    profile: &str,
) -> Result<PathBuf, BackendError> {
    let filename = runtime_filename(windows);
    if let Some(explicit) = explicit {
        let explicit = PathBuf::from(explicit);
        let candidate = if explicit.is_dir() {
            explicit.join(filename)
        } else {
            explicit
        };
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(not_found_error(Some(&candidate)));
    }

    let mut candidates = Vec::new();
    if let Some(parent) = current_executable.parent() {
        candidates.push(parent.join(filename));
        candidates.push(parent.join("../lib/doria").join(filename));
        if let Some(profile_directory) = parent.parent() {
            candidates.push(profile_directory.join(filename));
        }
    }

    let target_root = target_override.map_or_else(
        || workspace.join("target"),
        |target| {
            let target = PathBuf::from(target);
            if target.is_absolute() {
                target
            } else {
                workspace.join(target)
            }
        },
    );
    candidates.push(target_root.join(profile).join(filename));
    let alternate_profile = if profile == "debug" {
        "release"
    } else {
        "debug"
    };
    candidates.push(target_root.join(alternate_profile).join(filename));

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| not_found_error(None))
}

fn runtime_filename(windows: bool) -> &'static str {
    if windows {
        "doria_rt.lib"
    } else {
        "libdoria_rt.a"
    }
}

fn not_found_error(path: Option<&Path>) -> BackendError {
    let detail = path
        .map(|path| format!(" at `{}`", path.display()))
        .unwrap_or_default();
    BackendError::new(format!(
        "doria-rt static library was not found{detail}\nhelp: build it with `cargo build -p doria-rt` or set DORIA_RT_PATH"
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_directory(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "doriac-runtime-artifact-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary directory should be created");
        path
    }

    #[test]
    fn explicit_runtime_path_wins() {
        let directory = temp_directory("override");
        let runtime = directory.join(runtime_filename(false));
        fs::write(&runtime, b"archive").expect("runtime fixture should be written");
        let resolved = resolve(
            Some(runtime.as_os_str()),
            &directory.join("bin/doriac"),
            &directory,
            None,
            false,
            "debug",
        )
        .expect("explicit runtime should resolve");
        assert_eq!(resolved, runtime);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn development_target_directory_is_a_fallback() {
        let directory = temp_directory("target");
        let runtime = directory.join("target/debug").join(runtime_filename(false));
        fs::create_dir_all(runtime.parent().expect("runtime should have parent"))
            .expect("target directory should be created");
        fs::write(&runtime, b"archive").expect("runtime fixture should be written");
        let resolved = resolve(
            None,
            &directory.join("elsewhere/doriac"),
            &directory,
            None,
            false,
            "debug",
        )
        .expect("workspace runtime should resolve");
        assert_eq!(resolved, runtime);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn missing_runtime_has_build_help() {
        let directory = temp_directory("missing");
        let error = resolve(
            None,
            &directory.join("bin/doriac"),
            &directory,
            None,
            false,
            "debug",
        )
        .expect_err("missing runtime should fail");
        assert!(error
            .message
            .contains("doria-rt static library was not found"));
        assert!(error.message.contains("cargo build -p doria-rt"));
        assert!(error.message.contains("DORIA_RT_PATH"));
        let _ = fs::remove_dir_all(directory);
    }
}
