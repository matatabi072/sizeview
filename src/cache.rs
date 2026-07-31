//! フォルダサイズの永続キャッシュ。
//!
//! - 保存先: %LOCALAPPDATA%\sizeview\cache.json
//! - キー: フォルダ絶対パス (String)
//! - 検証: 保存時の folder mtime と現行の folder mtime が一致すれば hit
//!   (NTFS 上、フォルダ mtime は直下エントリの追加/削除/リネームで更新。
//!    深いところのファイル書き換えは反映されない = 弱い検証。強制再計算は refresh ボタンで)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 1;
/// これを超えると新規挿入時に一部を破棄 (HashMap の任意順)。厳密な LRU ではなく
/// 上限保証のみ。1 エントリ数百 byte なので 10 万エントリでも 数十 MB 程度。
const CACHE_SOFT_CAP: usize = 100_000;
const CACHE_EVICT_RATIO: usize = 10; // 1/10 落とす

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CacheEntry {
    pub size: u64,
    pub mtime_unix: i64,
    #[serde(default)]
    pub err: bool,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: HashMap<String, CacheEntry>,
}

pub struct SizeCache {
    map: HashMap<String, CacheEntry>,
    dirty: bool,
    save_path: PathBuf,
}

impl SizeCache {
    pub fn load_default() -> Self {
        let save_path = default_cache_path();
        let map = read_cache(&save_path).unwrap_or_default();
        Self {
            map,
            dirty: false,
            save_path,
        }
    }

    /// mtime が一致するときだけ Some を返す。mtime 未知 (0) の場合は保存側/検索側とも 0 で扱うので偶発的に hit する — ドライブ直下等では諦める。
    pub fn get(&self, path: &Path, current_mtime_unix: i64) -> Option<(u64, bool)> {
        let key = path.to_str()?;
        let e = self.map.get(key)?;
        if e.mtime_unix == current_mtime_unix {
            Some((e.size, e.err))
        } else {
            None
        }
    }

    pub fn insert(&mut self, path: &Path, size: u64, mtime_unix: i64, err: bool) {
        if let Some(key) = path.to_str() {
            let ent = CacheEntry {
                size,
                mtime_unix,
                err,
            };
            match self.map.get(key) {
                Some(existing)
                    if existing.size == ent.size
                        && existing.mtime_unix == ent.mtime_unix
                        && existing.err == ent.err => {}
                _ => {
                    // 新規キーで cap 超過なら evict
                    if !self.map.contains_key(key) && self.map.len() >= CACHE_SOFT_CAP {
                        self.evict_some();
                    }
                    self.map.insert(key.to_string(), ent);
                    self.dirty = true;
                }
            }
        }
    }

    fn evict_some(&mut self) {
        let drop_count = (self.map.len() / CACHE_EVICT_RATIO).max(1);
        let victims: Vec<String> = self.map.keys().take(drop_count).cloned().collect();
        for k in victims {
            self.map.remove(&k);
        }
        self.dirty = true;
    }

    pub fn invalidate(&mut self, path: &Path) {
        if let Some(key) = path.to_str() {
            if self.map.remove(key).is_some() {
                self.dirty = true;
            }
        }
    }

    pub fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let cf = CacheFile {
            version: CACHE_VERSION,
            entries: self.map.clone(),
        };
        if let Ok(json) = serde_json::to_vec(&cf) {
            if crate::fsutil::atomic_write(&self.save_path, &json).is_ok() {
                self.dirty = false;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

fn default_cache_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("sizeview").join("cache.json")
}

fn read_cache(path: &Path) -> Option<HashMap<String, CacheEntry>> {
    let bytes = std::fs::read(path).ok()?;
    let cf: CacheFile = serde_json::from_slice(&bytes).ok()?;
    if cf.version != CACHE_VERSION {
        return None;
    }
    Some(cf.entries)
}
