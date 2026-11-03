use std::ffi::OsString;

use regex::bytes::Regex;

use crate::git::{InspectionError, StagedChanges, StagedFile};
use crate::suppressions;

pub struct Finding {
    pub path: OsString,
    pub line_number: usize,
    pub fingerprint: String,
}

pub fn find_home_paths(changes: StagedChanges) -> Result<Vec<Finding>, InspectionError> {
    let suppressed_fingerprints = suppressions::parse(&changes.ignore_file)?;
    let home_path = Regex::new(r#"(?-u:(?:^|[\s\"'`=(\[{,:])/(?:Users|home)/[^/\s\"'`<>\x00]+)"#)
        .expect("the home-path pattern is valid");
    let mut findings = Vec::new();

    for file in changes.files {
        findings.extend(find_file_home_paths(file, &home_path)?);
    }
    findings.retain(|finding| !suppressed_fingerprints.contains(&finding.fingerprint));
    Ok(findings)
}

fn find_file_home_paths(
    file: StagedFile,
    home_path: &Regex,
) -> Result<Vec<Finding>, InspectionError> {
    let mut findings = Vec::new();
    let mut line_number = None;

    for line in file.patch.split(|byte| *byte == b'\n') {
        if line.starts_with(b"@@ ") {
            line_number = Some(added_line_number(line)?);
            continue;
        }
        let Some(number) = line_number else {
            continue;
        };
        match line.first() {
            Some(b'+') => {
                if home_path.is_match(&line[1..]) {
                    findings.push(Finding {
                        path: file.path.clone(),
                        line_number: number,
                        fingerprint: suppressions::fingerprint(&file.path, number, &line[1..]),
                    });
                }
                line_number = Some(number.checked_add(1).ok_or(InspectionError::InvalidPatch)?);
            }
            Some(b' ') => {
                line_number = Some(number.checked_add(1).ok_or(InspectionError::InvalidPatch)?);
            }
            _ => {}
        }
    }
    Ok(findings)
}

fn added_line_number(hunk_header: &[u8]) -> Result<usize, InspectionError> {
    let added_range = hunk_header
        .split(|byte| *byte == b' ')
        .nth(2)
        .and_then(|range| range.strip_prefix(b"+"))
        .ok_or(InspectionError::InvalidPatch)?;
    let first_line = added_range
        .split(|byte| *byte == b',')
        .next()
        .ok_or(InspectionError::InvalidPatch)?;
    std::str::from_utf8(first_line)
        .ok()
        .and_then(|number| number.parse().ok())
        .ok_or(InspectionError::InvalidPatch)
}
