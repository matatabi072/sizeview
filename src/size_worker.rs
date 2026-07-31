use eframe::egui;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub struct SizeResult {
    pub path: PathBuf,
    pub size: u64,
    pub err: bool,
    /// リクエスト時に渡された folder mtime。キャッシュ保存で使う。
    pub mtime_unix: i64,
    pub generation: u64,
}

pub struct SizeWorker {
    tx_res: Sender<SizeResult>,
    rx_res: Receiver<SizeResult>,
    current_gen: Arc<AtomicU64>,
    ctx: egui::Context,
}

impl SizeWorker {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx_res, rx_res) = channel::<SizeResult>();
        Self {
            tx_res,
            rx_res,
            current_gen: Arc::new(AtomicU64::new(0)),
            ctx,
        }
    }

    pub fn bump_generation(&self) -> u64 {
        self.current_gen.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current_generation(&self) -> u64 {
        self.current_gen.load(Ordering::SeqCst)
    }

    pub fn request(&self, path: PathBuf, mtime_unix: i64, generation: u64) {
        let tx = self.tx_res.clone();
        let gen = Arc::clone(&self.current_gen);
        let ctx = self.ctx.clone();
        rayon::spawn(move || {
            if generation < gen.load(Ordering::Relaxed) {
                return;
            }
            let err = AtomicBool::new(false);
            let size = compute_dir_size(&path, &gen, generation, &err);
            if generation < gen.load(Ordering::Relaxed) {
                return;
            }
            let _ = tx.send(SizeResult {
                path,
                size,
                err: err.into_inner(),
                mtime_unix,
                generation,
            });
            ctx.request_repaint();
        });
    }

    pub fn try_drain(&self) -> Vec<SizeResult> {
        let mut out = Vec::new();
        while let Ok(r) = self.rx_res.try_recv() {
            out.push(r);
        }
        out
    }
}

fn compute_dir_size(
    dir: &Path,
    current_gen: &AtomicU64,
    my_gen: u64,
    err: &AtomicBool,
) -> u64 {
    if my_gen < current_gen.load(Ordering::Relaxed) {
        return 0;
    }

    #[cfg(windows)]
    let raws = crate::winfs::list_dir(dir);
    #[cfg(not(windows))]
    let raws: Vec<crate::winfs_stub::RawEntry> = Vec::new();

    if raws.is_empty() {
        // ディレクトリは開けたが空、または開けなかった。ここでは空扱い (open エラーは winfs 内で 0 返しなので区別しない)。
        return 0;
    }

    let mut files_size: u64 = 0;
    let mut subdirs: Vec<PathBuf> = Vec::new();

    for r in raws {
        if r.is_reparse {
            continue;
        }
        if r.is_dir {
            subdirs.push(dir.join(&r.name));
        } else {
            files_size = files_size.saturating_add(r.size);
        }
    }

    if subdirs.is_empty() {
        return files_size;
    }

    let sub_total: u64 = subdirs
        .par_iter()
        .map(|p| compute_dir_size(p, current_gen, my_gen, err))
        .sum();

    files_size.saturating_add(sub_total)
}
