use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub type Result<T> = std::result::Result<T, AppError>;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub action: String,
}
impl AppError {
    pub fn new(code: &str, message: impl Into<String>, action: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            action: action.into(),
        }
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AppError {}
impl From<std::io::Error> for AppError {
    fn from(_: std::io::Error) -> Self {
        Self::new(
            "storage",
            "Could not read or write local data.",
            "Check folder permissions and available space.",
        )
    }
}
impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        Self::new(
            "config",
            "Local app data is invalid.",
            "Restore the config backup before continuing.",
        )
    }
}
impl From<git2::Error> for AppError {
    fn from(e: git2::Error) -> Self {
        Self::new("git", format!("Git operation failed ({:?}/{:?}).", e.class(), e.code()), "Check connectivity, token access, and repository state. Your recovery backups remain available.")
    }
}
#[cfg(feature = "desktop")]
impl From<tauri::Error> for AppError {
    fn from(_: tauri::Error) -> Self {
        Self::new(
            "window",
            "Could not update the app window.",
            "Try moving the window to another screen.",
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub enum Automation {
    #[default]
    Off,
    AfterChanges,
    FolderIdle,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct RemoteRepository {
    pub id: String,
    pub full_name: String,
    pub owner: String,
    pub private: bool,
    pub writable: bool,
    pub default_branch: String,
}
impl RemoteRepository {
    pub fn url(&self) -> String {
        format!("https://github.com/{}.git", self.full_name)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct RepositoryMapping {
    pub remote: RemoteRepository,
    pub folder: String,
    pub branch: String,
    pub last_sync: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct PendingSetup {
    pub name: String,
    pub description: String,
    pub folder: String,
    pub remote: Option<RemoteRepository>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct Settings {
    pub version: u32,
    pub repositories: Vec<RepositoryMapping>,
    pub active_id: Option<String>,
    pub device_name: String,
    pub commit_name: String,
    pub commit_email: String,
    pub automation: Automation,
    pub status_interval_secs: u32,
    pub fetch_interval_secs: u32,
    pub pending_setup: Option<PendingSetup>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            repositories: vec![],
            active_id: None,
            device_name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or("My device".into()),
            commit_name: String::new(),
            commit_email: String::new(),
            automation: Automation::Off,
            status_interval_secs: 10,
            fetch_interval_secs: 60,
            pending_setup: None,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct DiffCounts {
    pub added: u32,
    pub modified: u32,
    pub deleted: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct StatusSnapshot {
    pub version: u32,
    pub sequence: u32,
    pub repository_id: Option<String>,
    pub phase: String,
    pub label: String,
    pub connectivity: String,
    pub ahead: u32,
    pub behind: u32,
    pub changes: DiffCounts,
    pub last_sync: Option<String>,
    pub recovery: bool,
    pub error: Option<AppError>,
}
impl Default for StatusSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            sequence: 0,
            repository_id: None,
            phase: "idle".into(),
            label: "Choose a repository".into(),
            connectivity: "unknown".into(),
            ahead: 0,
            behind: 0,
            changes: DiffCounts::default(),
            last_sync: None,
            recovery: false,
            error: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct Commit {
    pub oid: String,
    pub summary: String,
    pub author: String,
    pub timestamp: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct FileDiff {
    pub path: String,
    pub binary: bool,
    pub truncated: bool,
    pub patch: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct ComparisonFile {
    pub path: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct ComparisonList {
    pub files: Vec<ComparisonFile>,
    pub cloud_oid: Option<String>,
    pub branch: String,
    pub checked_at: String,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct FileComparison {
    pub path: String,
    pub local: Option<String>,
    pub cloud: Option<String>,
    pub binary: bool,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct IgnoreSettings {
    pub patterns: Vec<String>,
    pub existing: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct FolderEntry {
    pub path: String,
    pub directory: bool,
    pub tracked: bool,
    pub ignored: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct Conflict {
    pub path: String,
    pub local: Option<String>,
    pub remote: Option<String>,
    pub binary: bool,
    pub truncated: bool,
    pub resolved: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/generated/")]
pub struct Identity {
    pub login: String,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    pub version: u32,
    pub operation_id: String,
    pub repository_id: String,
    pub folder: String,
    pub branch: String,
    pub phase: String,
    pub original_head: Option<String>,
    pub backup_ref: String,
    pub stash_oid: Option<String>,
    pub expected: String,
    pub in_flight: bool,
    pub retries: u8,
    pub conflicts: Vec<Conflict>,
    pub completed: bool,
}
