use crate::{git, models::*, storage::atomic_write};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const START: &str = "# lazysync: ignored paths (begin)";
const END: &str = "# lazysync: ignored paths (end)";

fn read(root: &Path) -> Result<String> {
    let path = git::safe_path(root, ".gitignore")?;
    match fs::read(&path) {
        Ok(bytes) => {
            if bytes.len() > 262144 {
                return Err(AppError::new(
                    "ignoreSize",
                    ".gitignore is too large to edit here.",
                    "Edit it in your text editor.",
                ));
            }
            String::from_utf8(bytes).map_err(|_| {
                AppError::new(
                    "ignoreEncoding",
                    ".gitignore must be UTF-8.",
                    "Edit it in your text editor.",
                )
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}
fn split(text: &str) -> Result<(String, Vec<String>)> {
    let mut existing = String::new();
    let mut patterns = vec![];
    let mut inside = false;
    let mut seen = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == START {
            if seen {
                return Err(AppError::new(
                    "ignoreBlock",
                    "Duplicate lazysync ignore sections.",
                    "Review .gitignore in your editor.",
                ));
            }
            inside = true;
            seen = true;
        } else if trimmed == END {
            if !inside {
                return Err(AppError::new(
                    "ignoreBlock",
                    "Invalid lazysync ignore section.",
                    "Review .gitignore in your editor.",
                ));
            }
            inside = false;
        } else if inside {
            if !trimmed.is_empty() {
                patterns.push(trimmed.to_string());
            }
        } else {
            existing.push_str(line);
        }
    }
    if inside {
        return Err(AppError::new(
            "ignoreBlock",
            "Unclosed lazysync ignore section.",
            "Review .gitignore in your editor.",
        ));
    }
    Ok((existing, patterns))
}
pub fn settings(root: &Path) -> Result<IgnoreSettings> {
    let text = read(root)?;
    let (existing, patterns) = split(&text)?;
    Ok(IgnoreSettings {
        patterns,
        existing,
        revision: format!("{:x}", Sha256::digest(text.as_bytes())),
    })
}
pub fn validate(patterns: &[String]) -> Result<()> {
    if patterns.len() > 2000
        || patterns
            .iter()
            .any(|p| p.len() > 1024 || p.contains(['\n', '\r', '\0']) || p == START || p == END)
    {
        return Err(AppError::new(
            "ignorePatterns",
            "Ignore rules exceed the supported limits.",
            "Use one pattern per line, up to 2,000 rules.",
        ));
    }
    Ok(())
}
pub fn save(root: &Path, patterns: Vec<String>, revision: &str) -> Result<IgnoreSettings> {
    validate(&patterns)?;
    let original = read(root)?;
    split(&original)?;
    if format!("{:x}", Sha256::digest(original.as_bytes())) != revision {
        return Err(AppError::new(
            "ignoreChanged",
            ".gitignore changed while you were editing.",
            "Reload ignore settings before saving.",
        ));
    }
    let mut block = String::new();
    let patterns: Vec<_> = patterns.into_iter().filter(|s| !s.is_empty()).collect();
    if !patterns.is_empty() {
        block.push_str(START);
        block.push('\n');
        for pattern in patterns {
            block.push_str(&pattern);
            block.push('\n');
        }
        block.push_str(END);
        block.push('\n');
    }
    let mut start = None;
    let mut end = None;
    let mut offset = 0;
    for line in original.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == START {
            start = Some(offset);
        }
        if trimmed == END {
            end = Some(offset + line.len());
        }
        offset += line.len();
    }
    // Preserve the position of our section: later user negations must keep their precedence.
    let text = if let (Some(start), Some(end)) = (start, end) {
        format!("{}{}{}", &original[..start], block, &original[end..])
    } else {
        let newline = if !block.is_empty() && !original.is_empty() && !original.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        format!("{original}{newline}{block}")
    };
    if text.len() > 262144 {
        return Err(AppError::new(
            "ignoreSize",
            "The combined ignore rules are too large.",
            "Shorten the rules or edit .gitignore in your editor.",
        ));
    }
    let path = git::safe_path(root, ".gitignore")?;
    atomic_write(&path, text.as_bytes())?;
    settings(root)
}
pub fn entries(root: &Path, directory: &str) -> Result<Vec<FolderEntry>> {
    let folder = if directory.is_empty() {
        root.to_path_buf()
    } else {
        git::safe_path(root, directory)?
    };
    let repo = git::open(&root.to_string_lossy()).ok();
    let index = repo.as_ref().and_then(|r| r.index().ok());
    let mut result = vec![];
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(".git")
            || metadata.is_symlink()
        {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| {
                AppError::new(
                    "path",
                    "Invalid folder path.",
                    "Choose your repository folder.",
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let tracked = index.as_ref().is_some_and(|i| {
            i.iter().any(|e| {
                e.path == path.as_bytes()
                    || metadata.is_dir() && e.path.starts_with(format!("{path}/").as_bytes())
            })
        });
        let ignored = repo
            .as_ref()
            .is_some_and(|r| r.status_should_ignore(Path::new(&path)).unwrap_or(false));
        result.push(FolderEntry {
            path,
            directory: metadata.is_dir(),
            tracked,
            ignored,
        });
        if result.len() >= 500 {
            break;
        }
    }
    result.sort_by(|a, b| b.directory.cmp(&a.directory).then(a.path.cmp(&b.path)));
    Ok(result)
}
