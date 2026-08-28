//! The zellij client, minus its goodbye.
//!
//! On the way out zellij writes one teardown string: the escapes that put the
//! terminal back (kitty mode off, alternate screen off, cursor shown), then
//! `ESC [ rows ; 1 H` and a parting message. That cursor move is what hurts:
//! leaving the alternate screen has just restored the cursor to where the
//! shell was when the session started, and the move throws that away for the
//! bottom row, so the message plus its newline scroll the restored screen and
//! the next prompt appears under a screenful of blank lines. sinteractive
//! narrates the end of a session itself (`teardown_summary`), so the message
//! is noise on top of that.
//!
//! zellij writes it through [`ClientOsApi::get_stdout_writer`], which is the
//! one seam the client leaves us: [`Quiet`] wraps the real
//! [`ClientOsInputOutput`], forwards everything, and hands out a writer that
//! drops the message and the cursor move while keeping every restoring escape
//! around them. Errors and disconnect notices are not touched — only the two
//! messages a clean exit prints.

use std::io;
use std::path::Path;

use anyhow::Result;
use zellij_client::os_input_output::{AsyncSignals, AsyncStdin, ClientOsApi, ClientOsInputOutput};
use zellij_utils::data::Palette;
use zellij_utils::errors::ErrorContext;
use zellij_utils::ipc::{ClientToServerMsg, IpcReceiveError, ServerToClientMsg};
use zellij_utils::pane_size::Size;

/// What zellij says on a clean exit (`ExitReason::Normal` and
/// `ExitReason::NormalDetached`). Every other reason — an error, a forced
/// detach, a lost server — still reaches the terminal.
const GOODBYES: [&str; 2] = ["Bye from Zellij!", "Session detached"];

/// Leaving the alternate screen. Present in the teardown write and in no
/// render, so it tells a real goodbye from a pane that merely happens to be
/// showing the words (this file in an editor, say).
const EXIT_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";

/// A [`ClientOsApi`] that is the real one in every respect but its stdout.
#[derive(Debug, Clone)]
pub struct Quiet(pub ClientOsInputOutput);

impl ClientOsApi for Quiet {
    fn get_stdout_writer(&self) -> Box<dyn io::Write> {
        Box::new(QuietStdout(self.0.get_stdout_writer()))
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new(self.clone())
    }

    // Everything below is delegation.
    fn get_terminal_size(&self) -> Size {
        self.0.get_terminal_size()
    }
    fn set_raw_mode(&mut self) {
        self.0.set_raw_mode()
    }
    fn unset_raw_mode(&self) -> Result<(), io::Error> {
        self.0.unset_raw_mode()
    }
    fn get_stdin_reader(&self) -> Box<dyn io::BufRead> {
        self.0.get_stdin_reader()
    }
    fn stdin_is_terminal(&self) -> bool {
        self.0.stdin_is_terminal()
    }
    fn stdout_is_terminal(&self) -> bool {
        self.0.stdout_is_terminal()
    }
    fn update_session_name(&mut self, new_session_name: String) {
        self.0.update_session_name(new_session_name)
    }
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        self.0.read_from_stdin()
    }
    fn send_to_server(&self, msg: ClientToServerMsg) {
        self.0.send_to_server(msg)
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        self.0.recv_from_server()
    }
    fn try_recv_from_server(&self) -> Result<(ServerToClientMsg, ErrorContext), IpcReceiveError> {
        self.0.try_recv_from_server()
    }
    fn handle_signals(
        &self,
        sigwinch_cb: Box<dyn Fn()>,
        quit_cb: Box<dyn Fn()>,
        resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    ) {
        self.0.handle_signals(sigwinch_cb, quit_cb, resize_receiver)
    }
    fn connect_to_server(&self, path: &Path) {
        self.0.connect_to_server(path)
    }
    fn spawn_server(&self, socket_path: &Path, debug: bool) -> Result<(), io::Error> {
        self.0.spawn_server(socket_path, debug)
    }
    fn should_install_panic_hook(&self) -> bool {
        self.0.should_install_panic_hook()
    }
    fn load_palette(&self) -> Palette {
        self.0.load_palette()
    }
    fn enable_mouse(&self) -> Result<()> {
        self.0.enable_mouse()
    }
    fn disable_mouse(&self) -> Result<()> {
        self.0.disable_mouse()
    }
    fn restore_console_mode(&self) {
        self.0.restore_console_mode()
    }
    fn env_variable(&self, name: &str) -> Option<String> {
        self.0.env_variable(name)
    }
    fn get_async_stdin_reader(&self) -> Box<dyn AsyncStdin> {
        self.0.get_async_stdin_reader()
    }
    fn get_async_signal_listener(&self) -> io::Result<Box<dyn AsyncSignals>> {
        self.0.get_async_signal_listener()
    }
}

/// stdout with the goodbye taken out of it.
struct QuietStdout(Box<dyn io::Write>);

impl io::Write for QuietStdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match strip_goodbye(buf) {
            // The dropped bytes are still accounted for: the caller asked for
            // the whole buffer to go out and, as far as it is concerned, it
            // did.
            Some(kept) => self.0.write_all(&kept).map(|()| buf.len()),
            None => self.0.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// `buf` without the goodbye message, the cursor move that placed it and the
/// newline after it — or `None` when there is no goodbye in it.
fn strip_goodbye(buf: &[u8]) -> Option<Vec<u8>> {
    // The teardown is written on its own and is a hundred bytes at most, so
    // renders — the other thing coming through here, many times a second —
    // are ruled out on their length alone.
    if buf.len() > 512 || find(buf, EXIT_ALTERNATE_SCREEN).is_none() {
        return None;
    }
    let (at, len) = GOODBYES
        .iter()
        .find_map(|m| find(buf, m.as_bytes()).map(|i| (i, m.len())))?;
    let mut head = &buf[..at];
    // `ESC [ rows ; 1 H` sits immediately before the message. Dropping it
    // leaves the cursor where leaving the alternate screen put it.
    if let Some(esc) = head.iter().rposition(|&b| b == 0x1b) {
        if head[esc..].starts_with(b"\x1b[") && head.ends_with(b"H") {
            head = &head[..esc];
        }
    }
    let tail = &buf[at + len..];
    let tail = tail.strip_prefix(b"\n").unwrap_or(tail);
    Some([head, tail].concat())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// zellij's `terminal_teardown_message` for a 30-row terminal.
    fn teardown(message: &str) -> Vec<u8> {
        format!("\x1b[<1u\x1b[?2031l\x1b[?1004l\x1b[?1049l\x1b[m\x1b[?25h\x1b[30;1H{message}\n")
            .into_bytes()
    }

    #[test]
    fn the_goodbye_goes_and_the_restoring_escapes_stay() {
        let kept = strip_goodbye(&teardown("Bye from Zellij!")).expect("filtered");
        assert_eq!(
            kept,
            b"\x1b[<1u\x1b[?2031l\x1b[?1004l\x1b[?1049l\x1b[m\x1b[?25h"
        );
        let kept = strip_goodbye(&teardown("Session detached")).expect("filtered");
        assert!(!kept.ends_with(b"H"), "the cursor move goes too: {kept:?}");
        assert!(find(&kept, EXIT_ALTERNATE_SCREEN).is_some());
    }

    #[test]
    fn errors_and_renders_are_left_alone() {
        // An exit reason we do want on screen.
        assert!(strip_goodbye(&teardown("Session was detached from this client")).is_none());
        // A pane that merely shows the words — no teardown, no filtering.
        let render = b"\x1b[1;1Hlet exit_msg = String::from(\"Bye from Zellij!\");";
        assert!(strip_goodbye(render).is_none());
        assert!(strip_goodbye(b"").is_none());
    }
}
