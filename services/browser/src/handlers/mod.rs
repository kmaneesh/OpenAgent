//! Tool handler modules — one sub-module per functional area.
//!
//! All `handle_*` functions are re-exported here so `main` can import them
//! with a single `use crate::handlers::*;`.

mod config;
mod interaction;
mod page;
mod query;
mod storage;
mod tabs;

pub use config::{handle_dialog, handle_frame, handle_mouse, handle_network, handle_set};
pub use interaction::{
    handle_check, handle_click, handle_dblclick, handle_drag, handle_fill, handle_find,
    handle_focus, handle_hover, handle_keydown, handle_keyup, handle_press, handle_scroll,
    handle_scrollinto, handle_select, handle_type, handle_upload,
};
pub use page::{
    handle_back, handle_close, handle_close_all, handle_forward, handle_navigate, handle_open,
    handle_reload,
};
pub use query::{
    handle_console, handle_diff, handle_errors, handle_eval, handle_extract, handle_get,
    handle_highlight, handle_is, handle_screenshot, handle_snapshot, handle_wait,
};
pub use storage::{handle_cookies, handle_pdf, handle_state, handle_storage};
pub use tabs::{handle_tab_close, handle_tab_list, handle_tab_new, handle_tab_switch};
