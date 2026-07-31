use crate::config::Theme;
use eframe::egui;

/// System を解決した「実際に適用するテーマ」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effective {
    Dark,
    Light,
}

pub fn resolve(theme: Theme) -> Effective {
    match theme {
        Theme::Dark => Effective::Dark,
        Theme::Light => Effective::Light,
        Theme::System => {
            #[cfg(windows)]
            {
                if is_system_dark() {
                    Effective::Dark
                } else {
                    Effective::Light
                }
            }
            #[cfg(not(windows))]
            {
                Effective::Dark
            }
        }
    }
}

/// テーマを適用する。タイトルバーの再描画に失敗した場合 (窓ハンドル未取得等) は
/// false を返す — 呼び出し側は次フレームで再試行するのが望ましい。
pub fn apply(ctx: &egui::Context, theme: Theme) -> bool {
    let eff = resolve(theme);
    match eff {
        Effective::Dark => ctx.set_visuals(egui::Visuals::dark()),
        Effective::Light => ctx.set_visuals(egui::Visuals::light()),
    }
    #[cfg(windows)]
    {
        apply_titlebar(matches!(eff, Effective::Dark))
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize
///   AppsUseLightTheme (DWORD): 1=Light, 0=Dark
#[cfg(windows)]
pub fn is_system_dark() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        REG_DWORD, REG_VALUE_TYPE,
    };

    let subkey: Vec<u16> = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value_name: Vec<u16> = "AppsUseLightTheme"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_err()
        {
            return true; // 取れなければ Dark 扱い
        }

        let mut data: u32 = 1;
        let mut size: u32 = 4;
        let mut ty = REG_VALUE_TYPE(0);
        let r = RegQueryValueExW(
            hkey,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut ty),
            Some(&mut data as *mut _ as *mut u8),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);

        // REG_DWORD 以外や 4byte 未満は信用しない
        if r.is_err() || ty != REG_DWORD || size != 4 {
            return true;
        }
        data == 0 // 0=Dark, 1=Light
    }
}

/// SizeView 自身のトップレベル窓ハンドルを探す。
/// `GetForegroundWindow` は他アプリの窓を返す危険があるので使わない (pitfall #12 補足)。
#[cfg(windows)]
fn find_self_hwnd() -> windows::Win32::Foundation::HWND {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    let title_w: Vec<u16> = crate::build_info::WINDOW_TITLE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        FindWindowW(PCWSTR::null(), PCWSTR(title_w.as_ptr()))
            .unwrap_or(windows::Win32::Foundation::HWND::default())
    }
}

/// pitfall #12: DwmSetWindowAttribute 単体では非クライアント領域が再描画されない
/// → SetWindowPos SWP_FRAMECHANGED で強制再描画
///
/// 戻り値: 窓ハンドルが取れて適用したら true。取れなければ false (次フレーム再試行推奨)。
#[cfg(windows)]
fn apply_titlebar(dark: bool) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    };
    let hwnd = find_self_hwnd();
    if hwnd.is_invalid() {
        return false;
    }
    unsafe {
        let value: u32 = if dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const _ as *const _,
            4,
        );
        let _ = SetWindowPos(
            hwnd,
            HWND::default(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    true
}
