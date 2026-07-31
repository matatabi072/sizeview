#[cfg(debug_assertions)]
pub const WINDOW_TITLE: &str = "SizeView [DEBUG]";
#[cfg(not(debug_assertions))]
pub const WINDOW_TITLE: &str = "SizeView";

#[allow(dead_code)]
#[cfg(debug_assertions)]
pub const MUTEX_NAME: &str = r"Global\SizeView_SingleInstance_Debug";
#[allow(dead_code)]
#[cfg(not(debug_assertions))]
pub const MUTEX_NAME: &str = r"Global\SizeView_SingleInstance";

pub const IS_DEBUG: bool = cfg!(debug_assertions);

pub const APP_ID: &str = "sizeview";
