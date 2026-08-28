//! `sinteractive __popup VIEW JOBID` — what the status bar's keybindings
//! open in a floating pane. `queue` (Ctrl+b q) is the live queue view;
//! `help`, `notices` and `monitor` are rendered inline by the plugin, so
//! this only tells a stray caller where they went.

use anyhow::Result;

use super::queue;
use crate::cli::{PopupView, QueueArgs};

pub fn run(view: PopupView, _job_id: u64) -> Result<i32> {
    match view {
        PopupView::Queue => queue::run(QueueArgs {
            watch: true,
            ..QueueArgs::default()
        }),
        PopupView::Help | PopupView::Notices | PopupView::Monitor => {
            println!("{view:?} is handled by the status bar.");
            Ok(0)
        }
    }
}
