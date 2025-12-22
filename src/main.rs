#![allow(dead_code)]
#![allow(unused)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Error, Write};
use termion::{event::Key, input::TermRead, raw::IntoRawMode};
use termion::screen::AlternateScreen;
use termion::screen::IntoAlternateScreen;

pub struct View<'a> {
    pub bufvec: &'a mut Vec<String>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub offset: usize,
    pub offcol: usize,
    pub terminal_w: usize,
    pub terminal_h: usize,
    pub endline: String,
    pub saved: bool,
}

const ESC: &str = "\x1b";
fn goto(row1: u16, col1: u16) -> String {
    format!("{ESC}[{row1};{col1}H")
}

const CLEAR_SCREEN: &str = "\x1b[2J";
const CLEAR_LINE: &str = "\x1b[2K";
const CURSOR_HIDE: &str = "\x1b[?25l";
const CURSOR_SHOW: &str = "\x1b[?25h";
const STYLE_RESET: &str = "\x1b[0m";
const STYLE_INVERT_ON: &str = "\x1b[7m";
const STYLE_INVERT_OFF: &str = "\x1b[27m";

fn main() -> io::Result<()> {
    let pathstr: String = match std::env::args().nth(1) {
        Some(x) => x,
        None => "Untitled".to_string(),
    };

    let mut working = match OpenOptions::new().read(true).write(true).open(&pathstr) {
        Ok(f) => f,
        Err(_) => File::create_new(&pathstr).expect("File creation error"),
    };

    let buffered = BufReader::new(&working);
    let mut buflines = Vec::<String>::new();

    for line in buffered.lines() {
        let ln = line.unwrap();
        buflines.push(ln.clone().replace("\t", "    "));
    }

    let termsize::Size { rows, cols } = termsize::get().unwrap();
    let mut screen: View = View {
        bufvec: &mut buflines,
        cursor_x: 0,
        cursor_y: 0,
        offset: 0,
        terminal_w: (cols as usize),
        terminal_h: (rows as usize),
        offcol: 0,
        endline: "Red editor v0.1.0".to_string(),
        saved: true,
    };

    let stdin = std::io::stdin();
    let stdout = io::stdout().into_raw_mode()?.into_alternate_screen()?;
    let mut screen_out = AlternateScreen::from(stdout);

    frame(&mut screen_out, &screen);

    let mut isctrx = false;

    for k in stdin.keys() {
        let k = k?;
        let keyh = key(k, &mut screen, isctrx);

        clamp(&mut screen);

        if keyh.2 {
            use std::io::{Seek, SeekFrom};

            working.seek(SeekFrom::Start(0))?;
            working.set_len(0)?;
            working
                .write_all(screen.bufvec.join("\n").as_bytes())
                .expect("no write");
            working.flush().expect("Error flushing");

            screen.saved = true;
            screen.endline = format!("Wrote to {}", &pathstr);
        }

        if !keyh.0 {
            if screen.saved || keyh.2 {
                break;
            } else {
                screen.endline = format!("{} not saved", &pathstr);
            }
        }

        isctrx = keyh.1;
        clamp(&mut screen);
        frame(&mut screen_out, &screen);
    }

    Ok(())
}

fn frame<W: Write>(out: &mut W, view: &View) {
    let mut screen: String = String::new();

    screen.push_str(CURSOR_HIDE);
    screen.push_str(&goto(1, 1));

    let rrows = view.terminal_h.saturating_sub(1);

    for n in 0..rrows {
        let i: usize = n + view.offset;

        screen.push_str(&goto((n + 1) as u16, 1));
        screen.push_str(CLEAR_LINE);

        if i >= view.bufvec.len() {
            continue;
        }

        let line = match view.bufvec.get(i) {
            Some(x) => x,
            None => view.bufvec.last().unwrap(),
        };

        let start = view.offcol.min(line.len());
        let end = (view.offcol + view.terminal_w).min(line.len());
        screen.push_str(&line[start..end].replace("\t", "    "));
    }

    screen.push_str(&goto(view.terminal_h as u16, 1));
    screen.push_str(CLEAR_LINE);
    screen.push_str(&view.endline);

    screen.push_str(CURSOR_SHOW);
    let scr_row = view.cursor_y.saturating_sub(view.offset) + 1;
    let scr_col = view.cursor_x.saturating_sub(view.offcol) + 1;
    screen.push_str(&goto(scr_row as u16, scr_col as u16));

    out.write_all(screen.as_bytes()).expect("Screen render error");
    out.flush().expect("Cannot flush screen");

}

fn key(k: Key, view: &mut View, ctrlx: bool) -> (bool, bool, bool) {
    if view.bufvec.is_empty() {
        view.bufvec.push(String::new());
        view.cursor_x = 0;
        view.cursor_y = 0;
        view.offset = 0;
        view.offcol = 0;
    }

    if view.cursor_y >= view.bufvec.len() {
        view.cursor_y = view.bufvec.len().saturating_sub(1);
    }

    let len = view.bufvec[view.cursor_y].len();
    if view.cursor_x > len {
        view.cursor_x = len;
    }

    if !ctrlx {
        match k {
            Key::Ctrl('z') => {
                return (false, false, true);
            }
            Key::Ctrl('x') => return (true, true, false),

            Key::Ctrl('n') | Key::Down => {
                // Down
                if view.cursor_y + 1 < view.bufvec.len() {
                    view.cursor_y += 1;
                    let len = view.bufvec[view.cursor_y].len();
                    view.cursor_x = view.cursor_x.min(len);
                }
            }

            Key::Ctrl('p') | Key::Up => {
                // Up
                if view.cursor_y > 0 {
                    view.cursor_y -= 1;
                    let len = view.bufvec[view.cursor_y].len();
                    view.cursor_x = view.cursor_x.min(len);
                }
            }

            Key::Ctrl('b') => {
                if view.cursor_x > 0 {
                    let line = &view.bufvec[view.cursor_y];
                    let bytes = line.as_bytes();

                    let can_jump4 = bytes
                        .get(view.cursor_x.saturating_sub(4)..view.cursor_x)
                        .is_some_and(|s| s.len() == 4 && s.iter().all(|&b| b == b' '));

                    let step = if can_jump4 { 4 } else { 1 };
                    view.cursor_x = view.cursor_x.saturating_sub(step);
                } else if view.cursor_y > 0 {
                    view.cursor_y -= 1;
                    view.cursor_x = view.bufvec[view.cursor_y].len();
                }
            }

            Key::Ctrl('f') => {
                let line = &view.bufvec[view.cursor_y];
                let bytes = line.as_bytes();
                let len = bytes.len();

                if view.cursor_x < len {
                    let can_jump4 = bytes
                        .get(view.cursor_x..view.cursor_x + 4)
                        .is_some_and(|s| s.iter().all(|&b| b == b' '));

                    view.cursor_x += if can_jump4 { 4 } else { 1 };
                } else if view.cursor_y + 1 < view.bufvec.len() {
                    view.cursor_y += 1;
                    view.cursor_x = 0;
                }
            }

            Key::Ctrl('a') => {
                view.cursor_x = 0;
            }

            Key::Ctrl('e') => {
                view.cursor_x = view.bufvec[view.cursor_y].len();
            }

            Key::Backspace => {
                let line = &view.bufvec[view.cursor_y];
                let bytes = line.as_bytes();
                let len = bytes.len();
                let can_jump4 = bytes
                    .get(view.cursor_x.saturating_sub(4)..view.cursor_x)
                    .is_some_and(|s| s.len() == 4 && s.iter().all(|&b| b == b' '));

                let moves = (if can_jump4 { 4 } else { 1 }).min(view.cursor_x);

                if view.cursor_x > 0 {
                    for k in 0..moves {
                        let line = &mut view.bufvec[view.cursor_y];
                        line.remove(view.cursor_x - 1);
                        view.cursor_x -= 1;
                    }
                } else if view.cursor_y > 0 {
                    let cur = view.bufvec.remove(view.cursor_y);
                    view.cursor_y -= 1;
                    view.cursor_x = view.bufvec[view.cursor_y].len();
                    view.bufvec[view.cursor_y].push_str(&cur);
                }

                view.saved = false;
            }
            Key::Char('\n') | Key::Char('\r') => {
                let cur_line = view.bufvec[view.cursor_y].clone();
                let (left, right) = cur_line.split_at(view.cursor_x);
                view.bufvec[view.cursor_y] = left.to_string();
                view.bufvec.insert(view.cursor_y + 1, right.to_string());
                view.cursor_y += 1;

                // Count leading indent in groups of 4 spaces (from the left part)
                let indent_levels = left
                    .as_bytes()
                    .chunks(4)
                    .take_while(|ch| ch.len() == 4 && ch.iter().all(|&b| b == b' '))
                    .count();
                let base = indent_levels * 4;

                // Special case: cursor was between { and } (right starts with })
                if right.trim_start().starts_with('}') {
                    let base_indent = " ".repeat(base);
                    let inner_indent = " ".repeat(base + 4);

                    // Current (new) line becomes the indented blank line
                    view.bufvec[view.cursor_y].clear();
                    view.bufvec[view.cursor_y].push_str(&inner_indent);
                    view.cursor_x = base + 4;

                    // Next line becomes the closing brace line
                    view.bufvec.insert(
                        view.cursor_y + 1,
                        format!("{}{}", base_indent, right.trim_start()),
                    );

                    view.saved = false;
                    // IMPORTANT: don't also apply the normal base indent after this
                    return (true, false, false);
                }

                // Normal case: just indent new line to base
                view.bufvec[view.cursor_y].insert_str(0, &" ".repeat(base));
                view.cursor_x = base;

                view.saved = false;
            }
            Key::Char('\t') => {
                for _ in 0..4 {
                    view.bufvec[view.cursor_y].insert(view.cursor_x, ' ');
                    view.cursor_x += 1;
                }
            }

            Key::Char('{') => {
                view.bufvec[view.cursor_y].insert_str(view.cursor_x, "{}");
                view.cursor_x += 1;
            }

            Key::Char(c) if !c.is_control() => {
                if view.bufvec.is_empty() {
                    view.bufvec.push(String::new());
                    view.cursor_y = 0;
                    view.cursor_x = 0;
                }
                if view.cursor_y >= view.bufvec.len() {
                    view.cursor_y = view.bufvec.len() - 1;
                    view.cursor_x = view.cursor_x.min(view.bufvec[view.cursor_y].len());
                }

                view.bufvec[view.cursor_y].insert(view.cursor_x, c);
                view.cursor_x += 1;
                view.saved = false;
            }

            _ => {}
        }
    } else {
        match k {
            Key::Ctrl('c') => return (false, false, false),
            Key::Ctrl('s') => return (true, false, true),
            _ => {}
        }
    }
    (true, false, false)
}

fn clamp(view: &mut View) {
    let text_h = view.terminal_h.saturating_sub(1);

    if view.cursor_y < view.offset {
        view.offset = view.cursor_y;
    }
    if text_h > 0 && view.cursor_y >= view.offset + text_h {
        view.offset = view.cursor_y + 1 - text_h;
    }

    if view.cursor_x < view.offcol {
        view.offcol = view.cursor_x;
    }
    if view.cursor_x >= view.offcol + view.terminal_w {
        view.offcol = view.cursor_x + 1 - view.terminal_w;
    }
}
