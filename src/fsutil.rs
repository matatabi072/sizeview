//! 汎用ファイル IO ヘルパ。

use std::io;
use std::path::Path;

/// tmp ファイルへ書いてから rename で置換。書き込み中クラッシュ耐性のため。
/// Windows の std::fs::rename は MOVEFILE_REPLACE_EXISTING 相当なので上書き可。
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);

    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
