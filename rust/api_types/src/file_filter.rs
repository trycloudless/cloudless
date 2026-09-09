/// Returns `true` if the given file path should be hidden from the file browser
/// by default because it is a system artifact or build-tool byproduct that users
/// typically do not interact with directly.
///
/// The check is path-component-based: a segment must match exactly (ignoring
/// ASCII case) rather than requiring the pattern to appear anywhere in the full
/// path. This prevents false positives — e.g. a user file called `build_notes.md`
/// will not be hidden even though the word "build" appears in it.
pub fn is_system_file(path: &str) -> bool {
    // Normalise to forward slashes for cross-platform consistency.
    let path = path.replace('\\', "/");

    // Check each path segment against the deny-list.
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        let seg_lower = segment.to_ascii_lowercase();

        // Exact-name matches (files or directories).
        if matches!(
            seg_lower.as_str(),
            ".ds_store"
                | ".localized"
                | "thumbs.db"
                | "desktop.ini"
                | "hiberfil.sys"
                | "pagefile.sys"
                | "swapfile.sys"
                | ".spotlight-v100"
                | ".trashes"
                | ".fseventsd"
                | ".temporaryitems"
                | "__macosx"
                | ".git"
                | ".svn"
                | ".hg"
                | "node_modules"
                | ".gradle"
                | ".next"
                | ".nuxt"
                | "__pycache__"
                | ".pytest_cache"
                | ".mypy_cache"
                | ".ruff_cache"
                | "meta-inf"
                | ".idea"
                | ".vscode"
        ) {
            return true;
        }

        // Extension-based matches.
        if matches!(
            seg_lower
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "pyc" | "pyo" | "class" | "o" | "obj"
        ) {
            return true;
        }
    }

    false
}

// ── File-type grouping ────────────────────────────────────────────────────────

/// Broad category a file belongs to, used for the file-browser filter pills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTypeGroup {
    All,
    Documents,
    Images,
    Videos,
    Audio,
    Archives,
}

/// Returns the `FileTypeGroup` for the given file path based on its extension.
/// Directories (no extension) and unrecognised extensions return `All`.
pub fn file_type_group(path: &str) -> FileTypeGroup {
    let ext = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        // Documents
        "pdf" | "doc" | "docx" | "odt" | "pages" | "rtf" | "txt" | "md" | "markdown" | "rst"
        | "tex" | "xls" | "xlsx" | "ods" | "numbers" | "csv" | "tsv" | "ppt" | "pptx" | "odp"
        | "key" | "epub" | "mobi" => FileTypeGroup::Documents,

        // Images
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tiff" | "tif"
        | "heic" | "heif" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "avif" | "jxl" => {
            FileTypeGroup::Images
        }

        // Videos
        "mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v" | "wmv" | "flv" | "3gp" | "ogv" | "ts"
        | "mts" | "m2ts" | "vob" => FileTypeGroup::Videos,

        // Audio
        "mp3" | "m4a" | "flac" | "wav" | "aac" | "ogg" | "wma" | "opus" | "aiff" | "aif"
        | "alac" | "ape" | "mid" | "midi" => FileTypeGroup::Audio,

        // Archives
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" | "rar" | "7z" | "dmg" | "iso" | "pkg"
        | "deb" | "rpm" => FileTypeGroup::Archives,

        _ => FileTypeGroup::All,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_macos_artifacts() {
        assert!(is_system_file(".DS_Store"));
        assert!(is_system_file("/Users/alice/docs/.DS_Store"));
        assert!(is_system_file("photos/__MACOSX/._image.jpg"));
        assert!(is_system_file(".Spotlight-V100"));
        assert!(is_system_file(".Trashes"));
    }

    #[test]
    fn hides_windows_artifacts() {
        assert!(is_system_file("Thumbs.db"));
        assert!(is_system_file("C:/Users/bob/docs/Thumbs.db"));
        assert!(is_system_file("desktop.ini"));
    }

    #[test]
    fn hides_vcs_directories() {
        assert!(is_system_file(".git/config"));
        assert!(is_system_file("project/.git/HEAD"));
        assert!(is_system_file(".svn/entries"));
        assert!(is_system_file("repo/.hg/store"));
    }

    #[test]
    fn hides_build_and_tool_directories() {
        assert!(is_system_file("node_modules/react/index.js"));
        assert!(is_system_file(".gradle/wrapper/gradle-wrapper.jar"));
        assert!(is_system_file("META-INF/MANIFEST.MF"));
        assert!(is_system_file("__pycache__/module.cpython-311.pyc"));
        assert!(is_system_file(
            ".next/cache/webpack/client-development/0.pack"
        ));
        assert!(is_system_file(".idea/workspace.xml"));
    }

    #[test]
    fn hides_compiled_bytecode_extensions() {
        assert!(is_system_file("app/main.pyc"));
        assert!(is_system_file("src/Foo.class"));
    }

    #[test]
    fn allows_user_files() {
        assert!(!is_system_file("documents/invoice.pdf"));
        assert!(!is_system_file("photos/vacation.jpg"));
        assert!(!is_system_file("src/main.rs"));
        assert!(!is_system_file("build_notes.md"));
        assert!(!is_system_file("my_node_module_list.txt"));
        assert!(!is_system_file("git_history_export.csv"));
    }

    #[test]
    fn case_insensitive() {
        assert!(is_system_file(".DS_STORE"));
        assert!(is_system_file("THUMBS.DB"));
        assert!(is_system_file("Desktop.Ini"));
        assert!(is_system_file("NODE_MODULES/foo"));
    }

    #[test]
    fn handles_backslash_paths() {
        assert!(is_system_file("C:\\Users\\bob\\Thumbs.db"));
        assert!(is_system_file("project\\.git\\config"));
    }

    #[test]
    fn file_type_group_documents() {
        assert_eq!(file_type_group("report.pdf"), FileTypeGroup::Documents);
        assert_eq!(file_type_group("notes.md"), FileTypeGroup::Documents);
        assert_eq!(file_type_group("data.csv"), FileTypeGroup::Documents);
        assert_eq!(file_type_group("slides.pptx"), FileTypeGroup::Documents);
    }

    #[test]
    fn file_type_group_images() {
        assert_eq!(file_type_group("photo.jpg"), FileTypeGroup::Images);
        assert_eq!(file_type_group("screenshot.PNG"), FileTypeGroup::Images);
        assert_eq!(file_type_group("raw.CR2"), FileTypeGroup::Images);
    }

    #[test]
    fn file_type_group_videos() {
        assert_eq!(file_type_group("movie.mp4"), FileTypeGroup::Videos);
        assert_eq!(file_type_group("clip.MOV"), FileTypeGroup::Videos);
    }

    #[test]
    fn file_type_group_audio() {
        assert_eq!(file_type_group("song.mp3"), FileTypeGroup::Audio);
        assert_eq!(file_type_group("track.FLAC"), FileTypeGroup::Audio);
    }

    #[test]
    fn file_type_group_archives() {
        assert_eq!(file_type_group("backup.zip"), FileTypeGroup::Archives);
        assert_eq!(file_type_group("data.tar.gz"), FileTypeGroup::Archives);
    }

    #[test]
    fn file_type_group_unknown_returns_all() {
        assert_eq!(file_type_group("main.rs"), FileTypeGroup::All);
        assert_eq!(file_type_group("Makefile"), FileTypeGroup::All);
        assert_eq!(file_type_group("no-extension"), FileTypeGroup::All);
    }

    #[test]
    fn file_type_group_uses_last_extension() {
        // "data.tar.gz" — last extension is "gz" → Archives
        assert_eq!(file_type_group("data.tar.gz"), FileTypeGroup::Archives);
        // "report.final.pdf" → Documents
        assert_eq!(
            file_type_group("report.final.pdf"),
            FileTypeGroup::Documents
        );
    }
}
