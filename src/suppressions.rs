use std::collections::HashSet;
use std::ffi::OsStr;

use sha2::{Digest, Sha256};

use crate::git::InspectionError;

pub fn fingerprint(path: &OsStr, line_number: usize, line: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(path.as_encoded_bytes());
    digest.update([0]);
    digest.update(line_number.to_string().as_bytes());
    digest.update([0]);
    digest.update(line);
    format!("home-path:{:x}", digest.finalize())
}

pub fn parse(contents: &[u8]) -> Result<HashSet<String>, InspectionError> {
    let contents = std::str::from_utf8(contents).map_err(|_| InspectionError::InvalidIgnoreFile)?;
    let mut fingerprints = HashSet::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (fingerprint, reason) = line
            .split_once('#')
            .ok_or(InspectionError::InvalidIgnoreFile)?;
        let fingerprint = fingerprint.trim();
        if reason.trim().is_empty() || !is_valid_fingerprint(fingerprint) {
            return Err(InspectionError::InvalidIgnoreFile);
        }
        fingerprints.insert(fingerprint.to_owned());
    }

    Ok(fingerprints)
}

fn is_valid_fingerprint(fingerprint: &str) -> bool {
    let Some(digest) = fingerprint.strip_prefix("home-path:") else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
