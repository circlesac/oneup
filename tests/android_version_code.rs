use chrono::{Datelike, Local};
use std::process::Command;

#[test]
fn gradle_cli_uses_four_digit_micro_and_rejects_exhaustion() {
    let now = Local::now();
    let yy = now.year() % 100;
    let mm = now.month();
    for (last_micro, extension, declaration) in [
        (99, "gradle", "versionName '0.0.0'\nversionCode 1\n"),
        (
            9998,
            "gradle.kts",
            "versionName = \"0.0.0\"\nversionCode = 1\n",
        ),
        (9999, "gradle", "versionName '0.0.0'\nversionCode 1\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(format!("build.{extension}"));
        std::fs::write(&target, declaration).unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec!["add", "."],
            vec!["commit", "--quiet", "-m", "fixture"],
            vec!["tag", &format!("v{yy}.{mm}.{last_micro}")],
        ] {
            let result = Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "Example User")
                .env("GIT_AUTHOR_EMAIL", "alice@example.com")
                .env("GIT_COMMITTER_NAME", "Example User")
                .env("GIT_COMMITTER_EMAIL", "alice@example.com")
                .output()
                .unwrap();
            assert!(result.status.success(), "{:?}", result);
        }
        let result = Command::new(env!("CARGO_BIN_EXE_oneup"))
            .args(["version", "--source", "git", "--target"])
            .arg(&target)
            .current_dir(dir.path())
            .output()
            .unwrap();
        let content = std::fs::read_to_string(&target).unwrap();
        if last_micro == 9999 {
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains("MICRO must be <= 9999"));
            assert_eq!(content, declaration);
        } else {
            assert!(result.status.success(), "{:?}", result);
            let version = format!("{yy}.{mm}.{}", last_micro + 1);
            assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), version);
            assert!(content.contains(&version));
            let code = yy as u32 * 1_000_000 + mm * 10_000 + last_micro + 1;
            assert!(content.contains(&format!(
                "versionCode {}{code}",
                if extension == "gradle.kts" { "= " } else { "" }
            )));
        }
    }
}
