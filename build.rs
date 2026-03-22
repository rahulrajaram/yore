use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=scripts/install-git-hooks.sh");
    println!("cargo:rerun-if-changed=.githooks/pre-commit");
    println!("cargo:rerun-if-changed=.githooks/commit-msg");
    println!("cargo:rerun-if-env-changed=COMMITHOOKS_DIR");
    println!("cargo:rerun-if-env-changed=YORE_INSTALL_GIT_HOOKS");

    let install_hooks = matches!(
        env::var("YORE_INSTALL_GIT_HOOKS")
            .ok()
            .as_deref()
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes")
    );
    if !install_hooks {
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    if manifest_dir.is_empty() {
        return;
    }

    let repo_root = Path::new(&manifest_dir);
    let install_script = repo_root.join("scripts/install-git-hooks.sh");
    if !install_script.is_file() {
        return;
    }

    let git_check = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(repo_root)
        .output();
    let Ok(git_check) = git_check else {
        return;
    };
    if !git_check.status.success() {
        return;
    }

    let script_check = Command::new("bash")
        .arg("-n")
        .arg(&install_script)
        .current_dir(repo_root)
        .status();
    match script_check {
        Ok(status) if status.success() => {}
        Ok(_) | Err(_) => {
            println!(
                "cargo:warning=Skipping commithooks install because scripts/install-git-hooks.sh failed shell validation"
            );
            return;
        }
    }

    let status = Command::new("bash")
        .arg(&install_script)
        .current_dir(repo_root)
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(_) => {
            println!(
                "cargo:warning=Commithooks install skipped; run ./scripts/install-git-hooks.sh manually if needed"
            );
        }
        Err(_) => {
            println!(
                "cargo:warning=Unable to execute scripts/install-git-hooks.sh; install hooks manually if needed"
            );
        }
    }
}
