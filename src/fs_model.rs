use std::path::{Path, PathBuf};

/// UI 上で扱う 1 レコード。
/// フォルダの場合 `size` は非同期計算完了までは `None`。
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    /// 名前の右に薄く表示する補足 (ドライブの空き/容量など)。
    pub info_suffix: Option<String>,
    pub path: PathBuf,
    pub is_dir: bool,
    /// ドライブ Entry かどうか (アイコン差替え等に使う)。
    pub is_drive: bool,
    pub size: Option<u64>,
    /// Unix 秒 (folder mtime)。取得不可なら None。
    pub mtime_unix: Option<i64>,
    pub kind_label: String,
}

/// 表示中の「場所」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Drives,
    Folder(PathBuf),
}

impl Location {
    pub fn display(&self) -> String {
        match self {
            Location::Drives => "PC".to_string(),
            Location::Folder(p) => p.display().to_string(),
        }
    }

    pub fn parent(&self) -> Option<Location> {
        match self {
            Location::Drives => None,
            Location::Folder(p) => match p.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => {
                    Some(Location::Folder(parent.to_path_buf()))
                }
                _ => Some(Location::Drives),
            },
        }
    }
}

pub fn enumerate_drives() -> Vec<Entry> {
    #[cfg(windows)]
    {
        use windows::Win32::Storage::FileSystem::GetLogicalDrives;
        let mask = unsafe { GetLogicalDrives() };
        let mut v = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{}:\\", letter);
            let info = crate::winfs::query_drive(&root);
            let name = match &info.label {
                Some(l) if !l.is_empty() => format!("{}: ({})", letter, l),
                _ => format!("{}:", letter),
            };
            let info_suffix = match (info.free_bytes, info.total_bytes) {
                (Some(free), Some(total)) => Some(format!(
                    "空き {} / {}",
                    humansize::format_size(free, humansize::BINARY),
                    humansize::format_size(total, humansize::BINARY),
                )),
                _ => None,
            };
            v.push(Entry {
                name,
                info_suffix,
                path: PathBuf::from(&root),
                is_dir: true,
                is_drive: true,
                size: None,
                mtime_unix: None,
                kind_label: info.kind.label().to_string(),
            });
        }
        v
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn read_folder(path: &Path) -> Vec<Entry> {
    #[cfg(windows)]
    {
        let raws = crate::winfs::list_dir(path);
        raws.into_iter()
            .map(|r| {
                let name = r.name.to_string_lossy().into_owned();
                let full = path.join(&r.name);
                let (size, kind) = if r.is_dir {
                    (None, "フォルダ".to_string())
                } else {
                    (Some(r.size), classify_file(&full))
                };
                Entry {
                    name,
                    info_suffix: None,
                    path: full,
                    is_dir: r.is_dir,
                    is_drive: false,
                    size,
                    mtime_unix: Some(r.mtime_unix),
                    kind_label: kind,
                }
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Vec::new()
    }
}

fn classify_file(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => format!("{} ファイル", ext.to_uppercase()),
        _ => "ファイル".to_string(),
    }
}
