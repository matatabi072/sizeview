//! SHGetFileInfoW でシステムアイコンを取得し、egui のテクスチャとしてキャッシュ。
//! - フォルダは 1 種類 (SHGFI_USEFILEATTRIBUTES で generic 取得)
//! - ドライブは文字ごと (実ディスクへ問い合わせ、custom icon も反映)
//! - 通常ファイルは拡張子ごと (USEFILEATTRIBUTES で disk hit 回避)

#![cfg(windows)]

use crate::fs_model::Entry;
use eframe::egui;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Folder,
    Drive(char),
    Ext(String),
}

pub struct IconCache {
    ctx: egui::Context,
    cache: HashMap<Key, Option<egui::TextureHandle>>,
}

impl IconCache {
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            ctx,
            cache: HashMap::new(),
        }
    }

    pub fn for_entry(&mut self, e: &Entry) -> Option<egui::TextureHandle> {
        let key = if e.is_drive {
            let letter = drive_letter(&e.path)?;
            Key::Drive(letter)
        } else if e.is_dir {
            Key::Folder
        } else {
            let ext = e
                .path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            Key::Ext(ext)
        };

        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let tex = extract(&self.ctx, &key);
        self.cache.insert(key, tex.clone());
        tex
    }
}

fn drive_letter(p: &Path) -> Option<char> {
    p.to_str()?.chars().next()
}

fn extract(ctx: &egui::Context, key: &Key) -> Option<egui::TextureHandle> {
    let (path_string, use_attrs, attrs) = match key {
        Key::Drive(letter) => {
            // 空 CD トレイ・切断済ネットワークドライブ等での UI ハング防止:
            // Fixed 以外は SHGetFileInfoW を実ディスクにかけない。None を返して
            // テーブルの emoji フォールバックに任せる。
            let kind = crate::winfs::drive_kind_of(*letter);
            if !matches!(kind, crate::winfs::DriveKind::Fixed) {
                return None;
            }
            (format!("{}:\\", letter), false, FILE_FLAGS_AND_ATTRIBUTES(0))
        }
        Key::Folder => (
            String::from("dummy_folder"),
            true,
            FILE_ATTRIBUTE_DIRECTORY,
        ),
        Key::Ext(ext) => {
            let s = if ext.is_empty() {
                "file".to_string()
            } else {
                format!("file.{}", ext)
            };
            (s, true, FILE_ATTRIBUTE_NORMAL)
        }
    };
    let path_w: Vec<u16> = OsStr::new(&path_string)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut sfi: SHFILEINFOW = unsafe { std::mem::zeroed() };
    let mut flags = SHGFI_ICON | SHGFI_SMALLICON;
    if use_attrs {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }
    let ret = unsafe {
        SHGetFileInfoW(
            PCWSTR(path_w.as_ptr()),
            attrs,
            Some(&mut sfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        )
    };
    if ret == 0 {
        return None;
    }
    let hicon = sfi.hIcon;
    if hicon.is_invalid() {
        return None;
    }

    let image = hicon_to_rgba(hicon);
    unsafe {
        let _ = DestroyIcon(hicon);
    }
    let (w, h, rgba) = image?;

    let color_image = egui::ColorImage {
        size: [w, h],
        pixels: rgba
            .chunks_exact(4)
            .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
            .collect(),
    };
    let id = match key {
        Key::Folder => "icon_folder".to_string(),
        Key::Drive(c) => format!("icon_drive_{}", c),
        Key::Ext(e) => format!("icon_ext_{}", e),
    };
    Some(ctx.load_texture(id, color_image, egui::TextureOptions::LINEAR))
}

fn hicon_to_rgba(hicon: HICON) -> Option<(usize, usize, Vec<u8>)> {
    unsafe {
        let mut ii: ICONINFO = std::mem::zeroed();
        if GetIconInfo(hicon, &mut ii).is_err() {
            return None;
        }
        let hbm_color = ii.hbmColor;
        let hbm_mask = ii.hbmMask;

        let mut bm: BITMAP = std::mem::zeroed();
        let n = GetObjectW(
            HGDIOBJ(hbm_color.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut _),
        );
        if n == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(HGDIOBJ(hbm_color.0));
            let _ = DeleteObject(HGDIOBJ(hbm_mask.0));
            return None;
        }
        let w = bm.bmWidth as usize;
        let h = bm.bmHeight as usize;

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32); // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB

        let mut buf = vec![0u8; w * h * 4];
        let hdc = GetDC(HWND::default());
        let ok = GetDIBits(
            hdc,
            hbm_color,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(HWND::default(), hdc);
        let _ = DeleteObject(HGDIOBJ(hbm_color.0));
        let _ = DeleteObject(HGDIOBJ(hbm_mask.0));
        if ok == 0 {
            return None;
        }

        // BGRA -> RGBA
        for chunk in buf.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        Some((w, h, buf))
    }
}
