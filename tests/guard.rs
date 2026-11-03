use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

struct Repository {
    directory: TempDir,
}

impl Repository {
    fn new() -> Self {
        let repository = Self {
            directory: tempfile::tempdir().expect("create temporary repository"),
        };
        repository.git(&["init", "--quiet"]);
        repository
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn write(&self, filename: &str, content: &str) {
        fs::write(self.path().join(filename), content).expect("write repository file");
    }

    fn git(&self, arguments: &[&str]) -> Output {
        let output = self.git_output(arguments);
        assert!(output.status.success(), "Git command failed: {arguments:?}");
        output
    }

    fn git_output(&self, arguments: &[&str]) -> Output {
        isolated_command(OsStr::new("git"), self.path())
            .args(arguments)
            .output()
            .expect("run Git")
    }

    fn guard(&self) -> Output {
        run_guard(self.path(), std::env::var_os("PATH").as_deref())
    }

    fn guard_with_fingerprints(&self) -> Output {
        isolated_command(OsStr::new(env!("CARGO_BIN_EXE_muttley")), self.path())
            .args(["guard", "--show-fingerprints"])
            .output()
            .expect("run guard with fingerprints")
    }

    fn commit(&self) {
        self.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=absent-hooks",
            "commit",
            "--quiet",
            "--no-verify",
            "-m",
            "Fixture",
        ]);
    }
}

fn run_guard(directory: &Path, search_path: Option<&OsStr>) -> Output {
    let mut command = isolated_command(OsStr::new(env!("CARGO_BIN_EXE_muttley")), directory);
    command.arg("guard");
    if let Some(search_path) = search_path {
        command.env("PATH", search_path);
    }
    command.output().expect("run muttley guard")
}

fn isolated_command(executable: &OsStr, directory: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .env_clear()
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", directory.join("absent-gitconfig"));
    if let Some(search_path) = std::env::var_os("PATH") {
        command.env("PATH", search_path);
    }
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SYSTEMROOT") {
        command.env("SYSTEMROOT", system_root);
    }
    command
}

fn synthetic_home_path(root: &str) -> String {
    ["", root, "synthetic-contributor", "checkout"].join("/")
}

fn reported_fingerprint(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("  fingerprint: "))
        .expect("report includes a fingerprint")
        .to_owned()
}

#[test]
fn suppression_does_not_hide_the_same_path_on_another_line() {
    let repository = Repository::new();
    let line = format!("source={}\n", synthetic_home_path("home"));
    repository.write("settings.txt", &line);
    repository.git(&["add", "settings.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    repository.write(
        ".muttleyignore",
        &format!("{fingerprint} # Deliberate first example.\n"),
    );
    repository.git(&["add", ".muttleyignore"]);
    repository.write("settings.txt", &format!("{line}{line}"));
    repository.git(&["add", "settings.txt"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).expect("UTF-8 report");
    assert!(report.starts_with("settings.txt:2:"), "{report}");
    assert_eq!(report.lines().count(), 1);
}

#[test]
fn an_unstaged_ignore_file_cannot_suppress_a_staged_finding() {
    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    repository.write(".muttleyignore", &format!("{fingerprint} # Not staged.\n"));

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("settings.txt:1:"));
}

#[test]
fn uses_the_staged_ignore_file_even_when_its_working_copy_is_invalid() {
    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    repository.write(
        ".muttleyignore",
        &format!("# Documented example.\n\n{fingerprint} # Intentional.\n"),
    );
    repository.git(&["add", ".muttleyignore"]);
    repository.write(".muttleyignore", "invalid unstaged configuration");

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn reports_changed_content_after_an_earlier_finding_was_suppressed() {
    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    repository.write(
        ".muttleyignore",
        &format!("{fingerprint} # Original example.\n"),
    );
    repository.git(&["add", ".muttleyignore"]);
    repository.write("settings.txt", &synthetic_home_path("Users"));
    repository.git(&["add", "settings.txt"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("settings.txt:1:"));
}

#[test]
fn suppression_does_not_hide_a_finding_in_another_file() {
    let repository = Repository::new();
    let home_path = synthetic_home_path("home");
    repository.write("first.txt", &home_path);
    repository.git(&["add", "first.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    repository.write(
        ".muttleyignore",
        &format!("{fingerprint} # First example only.\n"),
    );
    repository.git(&["add", ".muttleyignore"]);
    repository.write("second.txt", &home_path);
    repository.git(&["add", "second.txt"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).expect("UTF-8 report");
    assert!(report.starts_with("second.txt:1:"), "{report}");
    assert_eq!(report.lines().count(), 1);
}

#[test]
fn rejects_a_suppression_without_a_reason() {
    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    repository.write(".muttleyignore", &format!("{fingerprint} # \n"));
    repository.git(&["add", ".muttleyignore"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains(".muttleyignore"));
}

#[test]
fn rejects_a_malformed_suppression_without_echoing_its_contents() {
    let repository = Repository::new();
    let private_input = synthetic_home_path("home");
    repository.write(
        ".muttleyignore",
        &format!("home-path:{private_input} # Invalid entry.\n"),
    );
    repository.git(&["add", ".muttleyignore"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains(".muttleyignore"));
    assert!(!errors.contains("synthetic-contributor"));
}

#[test]
fn reads_unchanged_tracked_suppressions() {
    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    repository.write(
        ".muttleyignore",
        &format!("{fingerprint} # Deliberate example.\n"),
    );
    repository.git(&["add", ".muttleyignore"]);
    repository.git(&["rm", "--cached", "settings.txt"]);
    repository.commit();
    repository.git(&["add", "settings.txt"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn rejects_a_symlinked_ignore_file_without_reading_its_target() {
    use std::os::unix::fs::symlink;

    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    let fingerprint = reported_fingerprint(&repository.guard_with_fingerprints());
    let private_directory = tempfile::tempdir().expect("create external fixture directory");
    let target = private_directory.path().join("ignore.txt");
    fs::write(&target, format!("{fingerprint} # External suppression.\n"))
        .expect("write external fixture");
    symlink(&target, repository.path().join(".muttleyignore")).expect("create staged symlink");
    repository.git(&["add", ".muttleyignore"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains(".muttleyignore"));
    assert!(!errors.contains(&target.display().to_string()));
}

#[test]
fn suppresses_an_exact_finding_using_the_staged_ignore_file() {
    let repository = Repository::new();
    repository.write(
        "settings.json",
        &format!("{{\"source\":\"{}\"}}\n", synthetic_home_path("home")),
    );
    repository.git(&["add", "settings.json"]);

    let report = repository.guard_with_fingerprints();
    assert_eq!(report.status.code(), Some(1));
    let fingerprint = reported_fingerprint(&report);
    assert!(!String::from_utf8_lossy(&report.stdout).contains("synthetic-contributor"));
    repository.write(
        ".muttleyignore",
        &format!("{fingerprint} # Deliberate documentation example.\n"),
    );
    repository.git(&["add", ".muttleyignore"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn reports_an_inspection_error_when_git_is_missing() {
    let repository = Repository::new();

    let output = run_guard(repository.path(), Some(repository.path().as_os_str()));

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Git is unavailable"));
}

#[test]
fn reports_an_inspection_error_outside_a_git_repository() {
    let directory = tempfile::tempdir().expect("create non-repository directory");

    let output = run_guard(directory.path(), std::env::var_os("PATH").as_deref());

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not inspect"));
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains(&directory.path().display().to_string())
    );
}

#[test]
fn reports_an_inspection_error_when_the_index_is_corrupt() {
    let repository = Repository::new();
    fs::write(repository.path().join(".git/index"), b"not a Git index")
        .expect("write corrupt fixture index");

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not inspect"));
}

#[test]
fn ignores_relative_paths_and_system_paths() {
    let repository = Repository::new();
    repository.write(
        "settings.txt",
        "source=./checkout\ncommand=/usr/bin/env\nurl=https://example.invalid/home/docs\n",
    );
    repository.git(&["add", "settings.txt"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn inspection_does_not_change_the_index_or_working_copy() {
    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    repository.write("settings.txt", "source=./checkout\n");
    let index_before = fs::read(repository.path().join(".git/index")).expect("read index");
    let content_before =
        fs::read(repository.path().join("settings.txt")).expect("read working copy");

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read(repository.path().join(".git/index")).expect("read index"),
        index_before
    );
    assert_eq!(
        fs::read(repository.path().join("settings.txt")).expect("read working copy"),
        content_before
    );
}

#[test]
fn reports_added_linux_home_paths_at_their_staged_line_numbers() {
    let repository = Repository::new();
    let mut lines = (1..=20)
        .map(|number| format!("setting_{number}=safe"))
        .collect::<Vec<_>>();
    repository.write("settings.txt", &lines.join("\n"));
    repository.git(&["add", "settings.txt"]);
    repository.commit();
    let home_path = synthetic_home_path("home");
    lines[1] = format!("source={home_path}");
    lines[19] = format!("backup={home_path}");
    repository.write("settings.txt", &lines.join("\n"));
    repository.git(&["add", "settings.txt"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).expect("UTF-8 report");
    assert!(report.contains("settings.txt:2:"), "{report}");
    assert!(report.contains("settings.txt:20:"), "{report}");
    assert_eq!(report.lines().count(), 2);
    assert!(!report.contains("synthetic-contributor"));
}

#[test]
fn reports_a_literal_repository_relative_filename_when_run_from_a_subdirectory() {
    let repository = Repository::new();
    repository.write("[settings] file.txt", &synthetic_home_path("Users"));
    repository.git(&["add", "--", "[settings] file.txt"]);
    let subdirectory = repository.path().join("nested");
    fs::create_dir(&subdirectory).expect("create subdirectory");

    let output = run_guard(&subdirectory, std::env::var_os("PATH").as_deref());

    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).expect("UTF-8 report");
    assert!(report.starts_with("[settings] file.txt:1:"), "{report}");
    assert_eq!(report.lines().count(), 1);
}

#[test]
fn detects_home_paths_when_git_literal_pathspecs_are_enabled() {
    let repository = Repository::new();
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);

    let output = isolated_command(OsStr::new(env!("CARGO_BIN_EXE_muttley")), repository.path())
        .arg("guard")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .output()
        .expect("run guard with inherited Git settings");

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("settings.txt:1:"));
}

#[cfg(unix)]
#[test]
fn inspection_does_not_run_git_text_conversion_helpers() {
    let repository = Repository::new();
    repository.write(".gitattributes", "settings.txt diff=fixture\n");
    repository.write("settings.txt", &synthetic_home_path("home"));
    repository.git(&["add", "settings.txt"]);
    repository.git(&[
        "config",
        "diff.fixture.textconv",
        "sh -c 'printf ran > helper-ran'",
    ]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    assert!(!repository.path().join("helper-ran").exists());
}

#[test]
fn reports_the_staged_home_path_when_the_working_copy_has_been_corrected() {
    let repository = Repository::new();
    let home_path = synthetic_home_path("Users");
    repository.write(
        "settings.txt",
        &format!("# configuration\nsource={home_path}\n"),
    );
    repository.git(&["add", "settings.txt"]);
    repository.write("settings.txt", "# configuration\nsource=./checkout\n");

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).expect("UTF-8 report");
    assert!(report.contains("settings.txt:2"), "{report}");
    assert!(report.contains("home-path"), "{report}");
    let errors = String::from_utf8(output.stderr).expect("UTF-8 errors");
    assert!(!report.contains(&home_path));
    assert!(!errors.contains(&home_path));
    assert!(!report.contains("synthetic-contributor"));
    assert!(!errors.contains("synthetic-contributor"));
}

#[test]
fn ignores_a_home_path_that_exists_only_in_the_working_copy() {
    let repository = Repository::new();
    repository.write("settings.txt", "source=./checkout\n");
    repository.git(&["add", "settings.txt"]);
    repository.write("settings.txt", &synthetic_home_path("home"));

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn ignores_an_existing_home_path_when_an_unrelated_line_is_staged() {
    let repository = Repository::new();
    let home_path = synthetic_home_path("home");
    repository.write("settings.txt", &format!("source={home_path}\nmode=old\n"));
    repository.git(&["add", "settings.txt"]);
    repository.commit();
    repository.write("settings.txt", &format!("source={home_path}\nmode=new\n"));
    repository.git(&["add", "settings.txt"]);

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn reports_unresolved_conflicts_even_when_run_from_a_subdirectory() {
    let repository = Repository::new();
    repository.write("settings.txt", "mode=base\n");
    repository.git(&["add", "settings.txt"]);
    repository.commit();
    repository.git(&["checkout", "--quiet", "-b", "other"]);
    repository.write("settings.txt", "mode=other\n");
    repository.git(&["add", "settings.txt"]);
    repository.commit();
    repository.git(&["checkout", "--quiet", "-b", "current", "HEAD~1"]);
    repository.write("settings.txt", "mode=current\n");
    repository.git(&["add", "settings.txt"]);
    repository.commit();
    let merge = repository.git_output(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "merge",
        "other",
    ]);
    assert!(!merge.status.success(), "the fixture must have a conflict");

    let subdirectory = repository.path().join("nested");
    fs::create_dir(&subdirectory).expect("create subdirectory");
    let output = run_guard(&subdirectory, std::env::var_os("PATH").as_deref());

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unresolved conflicts"));
}

#[test]
fn passes_when_nothing_is_staged() {
    let repository = Repository::new();

    let output = repository.guard();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
