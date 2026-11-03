use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::process::Output;

pub struct StagedFile {
    pub path: OsString,
    pub patch: Vec<u8>,
}

pub struct StagedChanges {
    pub files: Vec<StagedFile>,
    pub ignore_file: Vec<u8>,
}

#[derive(Debug)]
pub enum InspectionError {
    GitUnavailable,
    GitFailed,
    UnresolvedConflicts,
    #[cfg(not(unix))]
    UnsupportedFilename,
    InvalidPatch,
    InvalidIgnoreFile,
}

impl fmt::Display for InspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::GitUnavailable => "Git is unavailable; install Git and make it available on PATH",
            Self::GitFailed => {
                "Git could not inspect staged changes; check the repository and index"
            }
            Self::UnresolvedConflicts => {
                "the index has unresolved conflicts; resolve them before inspection"
            }
            #[cfg(not(unix))]
            Self::UnsupportedFilename => "a staged filename could not be read on this platform",
            Self::InvalidPatch => "a staged diff could not be read; inspection is incomplete",
            Self::InvalidIgnoreFile => {
                "the staged .muttleyignore must be a regular UTF-8 file containing fingerprints with a reason after #"
            }
        };
        formatter.write_str(message)
    }
}

pub fn staged_changes(
    run_git: &impl Fn(&[&OsStr]) -> io::Result<Output>,
) -> Result<StagedChanges, InspectionError> {
    let arguments = [
        "ls-files",
        "--unmerged",
        "--full-name",
        "-z",
        "--",
        ":(top)**",
    ]
    .map(OsStr::new);
    if !git_output(run_git, &arguments)?.is_empty() {
        return Err(InspectionError::UnresolvedConflicts);
    }

    let arguments = [
        "diff",
        "--cached",
        "--name-only",
        "--diff-filter=AMT",
        "--no-renames",
        "--no-relative",
        "--no-ext-diff",
        "--no-textconv",
        "-z",
        "--",
    ]
    .map(OsStr::new);
    let filenames = git_output(run_git, &arguments)?;
    let mut staged_files = Vec::new();

    for filename in filenames.split(|byte| *byte == 0) {
        if filename.is_empty() {
            continue;
        }
        let path = filename_from_bytes(filename)?;
        let patch = staged_patch(run_git, &path)?;
        staged_files.push(StagedFile { path, patch });
    }

    Ok(StagedChanges {
        files: staged_files,
        ignore_file: staged_ignore_file(run_git)?,
    })
}

fn staged_ignore_file(
    run_git: &impl Fn(&[&OsStr]) -> io::Result<Output>,
) -> Result<Vec<u8>, InspectionError> {
    let arguments = [
        "ls-files",
        "--stage",
        "--full-name",
        "-z",
        "--",
        ":(top,literal).muttleyignore",
    ]
    .map(OsStr::new);
    let entry = git_output(run_git, &arguments)?;
    if entry.is_empty() {
        return Ok(Vec::new());
    }
    if !entry.starts_with(b"100644 ") && !entry.starts_with(b"100755 ") {
        return Err(InspectionError::InvalidIgnoreFile);
    }
    git_output(
        run_git,
        &[OsStr::new("show"), OsStr::new(":.muttleyignore")],
    )
}

fn staged_patch(
    run_git: &impl Fn(&[&OsStr]) -> io::Result<Output>,
    path: &OsStr,
) -> Result<Vec<u8>, InspectionError> {
    let mut pathspec = OsString::from(":(top,literal)");
    pathspec.push(path);
    let mut arguments = [
        "diff",
        "--cached",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-renames",
        "--no-relative",
        "--unified=0",
        "--ignore-submodules=all",
        "--output-indicator-new=+",
        "--output-indicator-old=-",
        "--output-indicator-context= ",
        "--",
    ]
    .map(OsStr::new)
    .to_vec();
    arguments.push(&pathspec);
    git_output(run_git, &arguments)
}

fn git_output(
    run_git: &impl Fn(&[&OsStr]) -> io::Result<Output>,
    arguments: &[&OsStr],
) -> Result<Vec<u8>, InspectionError> {
    let output = run_git(arguments).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => InspectionError::GitUnavailable,
        _ => InspectionError::GitFailed,
    })?;
    if !output.status.success() {
        return Err(InspectionError::GitFailed);
    }
    // Git diagnostics may contain local paths or file content.
    Ok(output.stdout)
}

#[cfg(unix)]
fn filename_from_bytes(filename: &[u8]) -> Result<OsString, InspectionError> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(filename.to_vec()))
}

#[cfg(not(unix))]
fn filename_from_bytes(filename: &[u8]) -> Result<OsString, InspectionError> {
    let filename =
        std::str::from_utf8(filename).map_err(|_| InspectionError::UnsupportedFilename)?;
    Ok(OsString::from(filename))
}
