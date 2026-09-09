/// Local-only preview of backup exclusion rules.
///
/// Walks the source directory and counts how many files and bytes would be
/// included or excluded by the given exclusion config. Nothing is persisted —
/// no server calls, no SQLite writes.
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use walkdir::WalkDir;

use api_types::backup_config::{
    PreviewBackupExclusionsRequest, PreviewBackupExclusionsResponse, PreviewExcludedFile,
};

use crate::{
    applications::backup::filesystem_scan::{compile_exclusions, normalize_relative_path},
    model::base::AppResult,
};

/// Walks the source directory and returns live inclusion/exclusion counts.
///
/// The function is intentionally `async fn` so Tauri can `await` it on its
/// async runtime, even though the walk itself is synchronous. For very large
/// trees a blocking task could be used, but the single-threaded overhead is
/// acceptable for a UI preview.
pub async fn preview_backup_exclusions(
    request: PreviewBackupExclusionsRequest,
) -> AppResult<PreviewBackupExclusionsResponse> {
    let compiled = compile_exclusions(&request.exclusions)?;
    let source_path = Path::new(&request.source_directory);
    let sample_limit = request.sample_limit.max(1);

    let included_files = AtomicUsize::new(0);
    let excluded_files = AtomicUsize::new(0);
    let included_bytes = AtomicU64::new(0);
    let excluded_bytes = AtomicU64::new(0);
    let mut sample_excluded: Vec<PreviewExcludedFile> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Unlike the backup scanner we do NOT use filter_entry here: we want to
    // count excluded entries, so we traverse into excluded directories too.
    let iter = WalkDir::new(&request.source_directory)
        .follow_links(false)
        .into_iter();

    for result in iter {
        match result {
            Err(e) => {
                errors.push(e.to_string());
            }
            Ok(entry) => {
                if entry.depth() == 0 {
                    continue;
                }

                let is_dir = entry.file_type().is_dir();
                let rel = normalize_relative_path(source_path, entry.path());

                if compiled.is_match(&rel, is_dir) {
                    if !is_dir {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        excluded_files.fetch_add(1, Ordering::Relaxed);
                        excluded_bytes.fetch_add(size, Ordering::Relaxed);
                        if sample_excluded.len() < sample_limit {
                            let matched_rule = compiled.matched_rule(&rel).map(str::to_owned);
                            sample_excluded.push(PreviewExcludedFile {
                                path: entry.path().to_string_lossy().into_owned(),
                                size,
                                matched_rule,
                            });
                        }
                    }
                } else if !is_dir {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    included_files.fetch_add(1, Ordering::Relaxed);
                    included_bytes.fetch_add(size, Ordering::Relaxed);
                }
            }
        }
    }

    Ok(PreviewBackupExclusionsResponse {
        included_files: included_files.into_inner(),
        excluded_files: excluded_files.into_inner(),
        included_bytes: included_bytes.into_inner(),
        excluded_bytes: excluded_bytes.into_inner(),
        sample_excluded_files: sample_excluded,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use api_types::backup_config::{
        BackupExclusionConfig, BackupExclusionEntry, BackupExclusionRule, BackupExclusionRuleKind,
    };

    fn write_file(dir: &Path, name: &str, content: &[u8]) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn test_preview_counts_included_and_excluded() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "report.pdf", b"binary data");
        write_file(tmp.path(), "notes.txt", b"some notes");
        write_file(tmp.path(), "installer.dmg", b"dmg content");

        let exclusions = BackupExclusionConfig {
            entries: vec![BackupExclusionEntry::Custom(BackupExclusionRule {
                id: "r1".into(),
                enabled: true,
                kind: BackupExclusionRuleKind::FileExtension,
                value: "dmg".into(),
            })],
        };

        let req = PreviewBackupExclusionsRequest {
            source_directory: tmp.path().to_string_lossy().into_owned(),
            exclusions,
            sample_limit: 10,
        };
        let resp = preview_backup_exclusions(req).await.unwrap();

        assert_eq!(resp.excluded_files, 1, "one .dmg should be excluded");
        assert_eq!(resp.included_files, 2, "pdf and txt should be included");
        assert_eq!(resp.sample_excluded_files.len(), 1);
        assert!(
            resp.sample_excluded_files[0]
                .path
                .ends_with("installer.dmg")
        );
    }

    #[tokio::test]
    async fn test_preview_empty_exclusions_includes_all() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.txt", b"a");
        write_file(tmp.path(), "b.txt", b"b");

        let req = PreviewBackupExclusionsRequest {
            source_directory: tmp.path().to_string_lossy().into_owned(),
            exclusions: BackupExclusionConfig::default(),
            sample_limit: 10,
        };
        let resp = preview_backup_exclusions(req).await.unwrap();

        assert_eq!(resp.included_files, 2);
        assert_eq!(resp.excluded_files, 0);
        assert!(resp.sample_excluded_files.is_empty());
    }

    #[tokio::test]
    async fn test_preview_sample_limit_respected() {
        let tmp = TempDir::new().unwrap();
        for i in 0..10 {
            write_file(tmp.path(), &format!("file{i}.tmp"), b"temp content");
        }

        let exclusions = BackupExclusionConfig {
            entries: vec![BackupExclusionEntry::Custom(BackupExclusionRule {
                id: "r1".into(),
                enabled: true,
                kind: BackupExclusionRuleKind::FileExtension,
                value: "tmp".into(),
            })],
        };

        let req = PreviewBackupExclusionsRequest {
            source_directory: tmp.path().to_string_lossy().into_owned(),
            exclusions,
            sample_limit: 3,
        };
        let resp = preview_backup_exclusions(req).await.unwrap();

        assert_eq!(resp.excluded_files, 10);
        assert_eq!(resp.sample_excluded_files.len(), 3);
    }

    #[tokio::test]
    async fn test_preview_excluded_bytes_counted() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "big.zip", &[0u8; 1024]);
        write_file(tmp.path(), "small.txt", b"hello");

        let exclusions = BackupExclusionConfig {
            entries: vec![BackupExclusionEntry::Custom(BackupExclusionRule {
                id: "r1".into(),
                enabled: true,
                kind: BackupExclusionRuleKind::FileExtension,
                value: "zip".into(),
            })],
        };

        let req = PreviewBackupExclusionsRequest {
            source_directory: tmp.path().to_string_lossy().into_owned(),
            exclusions,
            sample_limit: 10,
        };
        let resp = preview_backup_exclusions(req).await.unwrap();

        assert_eq!(resp.excluded_bytes, 1024);
        assert_eq!(resp.included_files, 1);
    }
}
