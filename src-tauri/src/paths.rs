use std::path::Path;

/// Existing paths are compared by filesystem identity after canonicalization.
/// Keep stored paths intact: verbatim Windows paths support long filenames.
pub fn same_folder(left: &str, right: &str) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => Path::new(left) == Path::new(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_of_existing_folder_match_without_modifying_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("spaced folder Unicode-ไทย");
        std::fs::create_dir(&root).unwrap();
        let original = root.to_str().unwrap();
        let canonical = std::fs::canonicalize(&root).unwrap();
        assert!(same_folder(original, canonical.to_str().unwrap()));
        assert!(same_folder(original, root.join(".").to_str().unwrap()));
        assert!(!same_folder(original, dir.path().to_str().unwrap()));
        assert!(root.is_dir());
    }
}
