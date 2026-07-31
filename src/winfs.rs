//! Windows 直接呼び出しによる高速ディレクトリ列挙。
//!
//! std::fs::read_dir も内部で FindFirstFileW を使うが、
//! - FIND_FIRST_EX_LARGE_FETCH フラグ (バッチ取得、syscall 数を数分の 1 に)
//! - FindExInfoBasic (短ファイル名 = 8.3 name を取得しない、若干高速)
//! を使わない。ここでは直接叩いて両方効かせる。

#![cfg(windows)]

use std::ffi::{c_void, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    FindClose, FindFirstFileExW, FindNextFileW, GetDiskFreeSpaceExW, GetDriveTypeW,
    GetVolumeInformationW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FIND_FIRST_EX_LARGE_FETCH, WIN32_FIND_DATAW,
};
use windows::Win32::Storage::FileSystem::{FindExInfoBasic, FindExSearchNameMatch};

/// 1 エントリの生情報 (WIN32_FIND_DATAW から必要な分だけ抽出)。
pub struct RawEntry {
    pub name: OsString,
    pub is_dir: bool,
    pub is_reparse: bool,
    pub size: u64,
    /// Unix epoch 秒。取れなければ 0。
    pub mtime_unix: i64,
}

/// dir 直下のエントリを列挙 (`.` `..` を除く)。エラー時は空 Vec。
pub fn list_dir(dir: &Path) -> Vec<RawEntry> {
    let wild = to_wide_wildcard(dir);
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe {
        FindFirstFileExW(
            PCWSTR(wild.as_ptr()),
            FindExInfoBasic,
            &mut data as *mut _ as *mut c_void,
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    let handle = match handle {
        Ok(h) if !h.is_invalid() => h,
        _ => return Vec::new(),
    };

    let mut out: Vec<RawEntry> = Vec::with_capacity(64);
    loop {
        if !is_dot_or_dotdot(&data.cFileName) {
            out.push(build_entry(&data));
        }
        if unsafe { FindNextFileW(handle, &mut data) }.is_err() {
            break;
        }
    }
    let _ = unsafe { FindClose(handle) };
    out
}

fn build_entry(d: &WIN32_FIND_DATAW) -> RawEntry {
    let name = wide_z_to_osstring(&d.cFileName);
    let attr = d.dwFileAttributes;
    let is_dir = attr & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let is_reparse = attr & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let size = ((d.nFileSizeHigh as u64) << 32) | (d.nFileSizeLow as u64);
    let mtime_unix = filetime_to_unix(d.ftLastWriteTime.dwLowDateTime, d.ftLastWriteTime.dwHighDateTime);
    RawEntry {
        name,
        is_dir,
        is_reparse,
        size,
        mtime_unix,
    }
}

fn is_dot_or_dotdot(buf: &[u16]) -> bool {
    match buf.first().copied() {
        Some(c) if c == b'.' as u16 => match buf.get(1).copied() {
            Some(0) => true,
            Some(c2) if c2 == b'.' as u16 => matches!(buf.get(2).copied(), Some(0)),
            _ => false,
        },
        _ => false,
    }
}

fn wide_z_to_osstring(buf: &[u16]) -> OsString {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..len])
}

/// FILETIME (1601-01-01 UTC 起点の 100ns 単位) → Unix 秒。
fn filetime_to_unix(low: u32, high: u32) -> i64 {
    let ticks = ((high as u64) << 32) | (low as u64);
    if ticks == 0 {
        return 0;
    }
    // 100ns 単位を秒に。1601→1970 は 11644473600 秒。
    let secs_since_1601 = (ticks / 10_000_000) as i64;
    secs_since_1601 - 11_644_473_600
}

/// ドライブ種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Cdrom,
    Remote,
    Ramdisk,
    Unknown,
}

impl DriveKind {
    pub fn label(self) -> &'static str {
        match self {
            DriveKind::Fixed => "ローカルディスク",
            DriveKind::Removable => "リムーバブルディスク",
            DriveKind::Cdrom => "CD/DVD ドライブ",
            DriveKind::Remote => "ネットワークドライブ",
            DriveKind::Ramdisk => "RAM ディスク",
            DriveKind::Unknown => "ドライブ",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub kind: DriveKind,
    pub label: Option<String>,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

/// GetDriveTypeW のみを呼ぶ軽量版 (物理アクセス無し)。アイコン抽出前の判定用。
pub fn drive_kind_of(letter: char) -> DriveKind {
    let root_w: [u16; 4] = [letter as u16, b':' as u16, b'\\' as u16, 0];
    unsafe {
        match GetDriveTypeW(PCWSTR(root_w.as_ptr())) {
            3 => DriveKind::Fixed,
            2 => DriveKind::Removable,
            5 => DriveKind::Cdrom,
            4 => DriveKind::Remote,
            6 => DriveKind::Ramdisk,
            _ => DriveKind::Unknown,
        }
    }
}

pub fn query_drive(root: &str) -> DriveInfo {
    let root_w: Vec<u16> = root
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let kind = unsafe {
        match GetDriveTypeW(PCWSTR(root_w.as_ptr())) {
            3 => DriveKind::Fixed,
            2 => DriveKind::Removable,
            5 => DriveKind::Cdrom,
            4 => DriveKind::Remote,
            6 => DriveKind::Ramdisk,
            _ => DriveKind::Unknown,
        }
    };

    // CD/DVD 等はメディア無しで問い合わせると遅い/固まる可能性あり → 空データ返却
    if matches!(kind, DriveKind::Cdrom | DriveKind::Removable) {
        return DriveInfo {
            kind,
            label: None,
            free_bytes: None,
            total_bytes: None,
        };
    }

    let label = get_volume_label(&root_w);
    let (free, total) = get_free_total(&root_w);
    DriveInfo {
        kind,
        label,
        free_bytes: free,
        total_bytes: total,
    }
}

fn get_volume_label(root_w: &[u16]) -> Option<String> {
    let mut name_buf = [0u16; 256];
    let ok = unsafe {
        GetVolumeInformationW(
            PCWSTR(root_w.as_ptr()),
            Some(&mut name_buf),
            None,
            None,
            None,
            None,
        )
    };
    if ok.is_err() {
        return None;
    }
    let len = name_buf.iter().position(|&c| c == 0).unwrap_or(name_buf.len());
    if len == 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&name_buf[..len]))
    }
}

fn get_free_total(root_w: &[u16]) -> (Option<u64>, Option<u64>) {
    let mut free_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(root_w.as_ptr()),
            Some(&mut free_to_caller),
            Some(&mut total),
            Some(&mut total_free),
        )
    };
    if ok.is_err() {
        (None, None)
    } else {
        (Some(free_to_caller), Some(total))
    }
}

/// `path` を `\\?\<abs>\*` (絶対パスかつドライブ形式のとき) の UTF-16 NUL 終端列に。
/// UNC や既に `\\?\` プレフィクスがある場合はプレフィクス付加しない。
fn to_wide_wildcard(path: &Path) -> Vec<u16> {
    let p_str: Vec<u16> = path.as_os_str().encode_wide().collect();
    let has_ext_prefix = p_str.len() >= 4
        && p_str[0] == b'\\' as u16
        && p_str[1] == b'\\' as u16
        && p_str[2] == b'?' as u16
        && p_str[3] == b'\\' as u16;
    let has_unc = p_str.len() >= 2
        && p_str[0] == b'\\' as u16
        && p_str[1] == b'\\' as u16
        && !has_ext_prefix;
    let has_drive_colon = p_str.len() >= 3 && p_str[1] == b':' as u16;

    let mut s = Vec::with_capacity(p_str.len() + 8);
    if !has_ext_prefix && !has_unc && has_drive_colon {
        // "\\?\" プレフィクス
        s.extend([
            b'\\' as u16,
            b'\\' as u16,
            b'?' as u16,
            b'\\' as u16,
        ]);
    }
    s.extend(&p_str);
    let last = s.last().copied();
    if last != Some(b'\\' as u16) && last != Some(b'/' as u16) {
        s.push(b'\\' as u16);
    }
    s.push(b'*' as u16);
    s.push(0);
    s
}
