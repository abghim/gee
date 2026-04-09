#![allow(dead_code)]
#![allow(unused)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Error, Write};
use termion::screen::AlternateScreen;
use termion::screen::IntoAlternateScreen;
use termion::{event::Key, input::TermRead, raw::IntoRawMode};

pub struct Status {
    pub saved: bool,
    pub quit: bool,
    pub ctrlx: bool,
    pub save: bool,
    pub forcequit: bool,
    pub selecting: bool,
}

pub struct View<'a> {
    pub working_col: usize,
    pub bufvec: &'a mut Vec<String>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub offset: usize,
    pub offcol: usize,
    pub terminal_w: usize,
    pub terminal_h: usize,
    pub mark: (usize, usize),
    pub endline: String,
    pub kill: String,
    pub status: Status,
}

impl<'a> View<'a> {
    fn trueloc(self: &Self) -> (usize, usize) {
        (self.cursor_x, self.cursor_y)
    }
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
        working_col: 0,
        cursor_x: 0,
        cursor_y: 0,
        offset: 0,
        mark: (0, 0),
        terminal_w: (cols as usize),
        terminal_h: (rows as usize),
        offcol: 0,
        endline: "".to_string(),
        kill: "Example kill text".to_string(),
        status: Status {
            saved: true,
            quit: false,
            ctrlx: false,
            save: false,
            forcequit: false,
            selecting: false,
        },
    };

    let stdin = std::io::stdin();
    let stdout = io::stdout().into_raw_mode()?.into_alternate_screen()?;
    let mut screen_out = AlternateScreen::from(stdout);

    frame(&mut screen_out, &screen);

    for k in stdin.keys() {
        let k = k?;
        key(k, &mut screen);
        let termsize::Size { rows, cols } = termsize::get().unwrap();
        screen.terminal_w = cols as usize;
        screen.terminal_h = rows as usize;

        clamp(&mut screen);

        if screen.status.selecting {
            screen.endline = "Selecting ".to_string();
        } else {
            screen.endline = "".to_string();
        }

        if screen.status.save {
            use std::io::{Seek, SeekFrom};

            working.seek(SeekFrom::Start(0))?;
            working.set_len(0)?;
            working
                .write_all(screen.bufvec.join("\n").as_bytes())
                .expect("no write");
            working.flush().expect("Error flushing");

            screen.status.saved = true;
            screen.endline = format!("Wrote to {}", &pathstr);
            screen.status.save = false;
        }

        if screen.status.quit {
            if screen.status.saved || screen.status.forcequit {
                break;
            } else {
                screen.endline = format!("{} not saved", &pathstr);
                screen.status.quit = false;
            }
        }
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

    let mut sel_start_x = 0;
    let mut sel_start_y = 0;
    let mut sel_end_x = 0;
    let mut sel_end_y = 0;

    if view.status.selecting {
        let cursor_before_mark = view.cursor_y < view.mark.1
            || (view.cursor_y == view.mark.1 && view.cursor_x <= view.mark.0);
        if cursor_before_mark {
            sel_start_x = view.cursor_x;
            sel_start_y = view.cursor_y;
            sel_end_x = view.mark.0;
            sel_end_y = view.mark.1;
        } else {
            sel_start_x = view.mark.0;
            sel_start_y = view.mark.1;
            sel_end_x = view.cursor_x;
            sel_end_y = view.cursor_y;
        }
    }

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

        if view.status.selecting
            && (i > sel_start_y || (i == sel_start_y && sel_start_x < line.len()))
            && (i < sel_end_y || (i == sel_end_y && sel_end_x > 0))
        {
            let line_len = line.len();
            let line_sel_start = if i == sel_start_y { sel_start_x } else { 0 };
            let line_sel_end = if i == sel_end_y { sel_end_x } else { line_len };
            let vis_sel_start = line_sel_start.max(start);
            let vis_sel_end = line_sel_end.min(end);

            if vis_sel_start < vis_sel_end {
                screen.push_str(&line[start..vis_sel_start].replace("\t", "    "));
                screen.push_str(STYLE_INVERT_ON);
                screen.push_str(&line[vis_sel_start..vis_sel_end].replace("\t", "    "));
                screen.push_str(STYLE_INVERT_OFF);
                screen.push_str(&line[vis_sel_end..end].replace("\t", "    "));
                continue;
            }
        }

        screen.push_str(&line[start..end].replace("\t", "    "));
    }

    screen.push_str(&goto(view.terminal_h as u16, 1));
    screen.push_str(CLEAR_LINE);
    screen.push_str(&view.endline);
    screen.push_str(CURSOR_SHOW);
    let scr_row = view.cursor_y.saturating_sub(view.offset) + 1;
    let scr_col = view.cursor_x.saturating_sub(view.offcol) + 1;
    screen.push_str(&goto(scr_row as u16, scr_col as u16));

    out.write_all(screen.as_bytes())
        .expect("Screen render error");
    out.flush().expect("Cannot flush screen");
}

fn key(k: Key, view: &mut View) {
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

    if !view.status.ctrlx {
        match k {
            Key::Ctrl('z') => {
                view.status.quit = true;
                view.status.save = true;
                return;
            }
            Key::Ctrl('x') => {
                view.status.ctrlx = true;
            }

            Key::Ctrl('n') | Key::Down => {
                // Down
                //
                view.working_col = view.working_col.max(view.cursor_x);
                if view.cursor_y + 1 < view.bufvec.len() {
                    view.cursor_y += 1;
                    let len = view.bufvec[view.cursor_y].len();
                    view.cursor_x = view.working_col.min(len);
                }
            }

            Key::Ctrl('p') | Key::Up => {
                // Up
                view.working_col = view.working_col.max(view.cursor_x);

                if view.cursor_y > 0 {
                    view.cursor_y -= 1;
                    let len = view.bufvec[view.cursor_y].len();
                    view.cursor_x = view.working_col.min(len);
                }
            }

            Key::Ctrl('b') | Key::Left => {
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

                view.working_col = view.cursor_x;
            }

            Key::Ctrl('f') | Key::Right => {
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

                view.working_col = view.cursor_x;
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

                view.status.saved = false;
                view.status.selecting = false;
            }

            Key::Ctrl('d') => {
                let line = &view.bufvec[view.cursor_y];
                let bytes = line.as_bytes();
                let len = bytes.len();
                let can_jump4 = bytes
                    .get(view.cursor_x..view.cursor_x + 4)
                    .is_some_and(|s| s.len() == 4 && s.iter().all(|&b| b == b' '));

                if view.cursor_x < len {
                    let moves = if can_jump4 { 4 } else { 1 };
                    for _ in 0..moves {
                        if view.cursor_x < view.bufvec[view.cursor_y].len() {
                            view.bufvec[view.cursor_y].remove(view.cursor_x);
                        }
                    }
                } else if view.cursor_y + 1 < view.bufvec.len() {
                    let next = view.bufvec.remove(view.cursor_y + 1);
                    view.bufvec[view.cursor_y].push_str(&next);
                }

                view.status.saved = false;
                view.status.selecting = false;
            }

            Key::Null | Key::Ctrl(' ') => {
                view.mark = view.trueloc();
                view.status.selecting = true;
            }

            Key::Ctrl('w') => {
                if view.status.selecting {
                    buf_kill_lines(view, view.mark);
                    view.status.selecting = false;
                }
                view.status.saved = false;
            }

            Key::Ctrl('y') => {
                buf_insert_lines(view, &view.kill.clone());
                view.status.saved = false;
            }

            Key::Ctrl('k') => {
                buf_kill_lines(view, (view.bufvec[view.cursor_y].len(), view.cursor_y));
                view.status.saved = false;
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

                if right.trim_start().starts_with('}') {
                    let base_indent = " ".repeat(base);
                    let inner_indent = " ".repeat(base + 4);

                    view.bufvec[view.cursor_y].clear();
                    view.bufvec[view.cursor_y].push_str(&inner_indent);
                    view.cursor_x = base + 4;

                    view.bufvec.insert(
                        view.cursor_y + 1,
                        format!("{}{}", base_indent, right.trim_start()),
                    );

                    view.status.saved = false;
                    view.status.selecting = false;
                    return;
                }

                view.bufvec[view.cursor_y].insert_str(0, &" ".repeat(base));
                view.cursor_x = base;

                view.status.saved = false;
                view.status.selecting = false;
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
                view.status.selecting = false;
                view.status.saved = false;
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
                view.status.saved = false;
                view.status.selecting = false;
            }

            _ => {}
        }
    } else {
        match k {
            Key::Ctrl('c') => {
                view.status.ctrlx = false;
                view.status.quit = true
            }
            Key::Ctrl('s') => {
                view.status.ctrlx = false;
                view.status.save = true;
            }
            Key::Char('x') => {
                view.status.forcequit = true;
                view.status.quit = true;
            }
            _ => {}
        }
    }
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

fn buf_insert_lines(view: &mut View, insert: &String) {
    // view.bufvec holds elements by lines. This logic can break if we are to insert a large string
    // with multiple lines. This function correctly handles multi-line-insertion by adding new rows
    // and splitting existing ones. It may be helpful to referece the 'enter' logic in key().
    // Replace the following todo!() with your code.
    let insert = insert.replace('\t', "    ");

    if insert.is_empty() {
        return;
    }

    if view.bufvec.is_empty() {
        view.bufvec.push(String::new());
        view.cursor_y = 0;
        view.cursor_x = 0;
    }

    if view.cursor_y >= view.bufvec.len() {
        view.cursor_y = view.bufvec.len() - 1;
    }
    view.cursor_x = view.cursor_x.min(view.bufvec[view.cursor_y].len());

    let parts: Vec<&str> = insert.split('\n').collect();
    if parts.len() == 1 {
        view.bufvec[view.cursor_y].insert_str(view.cursor_x, &insert);
        view.cursor_x += insert.len();
        view.status.saved = false;
        return;
    }

    let cur_line = view.bufvec[view.cursor_y].clone();
    let (left, right) = cur_line.split_at(view.cursor_x);

    let mut new_lines: Vec<String> = Vec::with_capacity(parts.len());
    new_lines.push(format!("{}{}", left, parts[0]));

    for segment in parts.iter().skip(1).take(parts.len().saturating_sub(2)) {
        new_lines.push((*segment).to_string());
    }

    new_lines.push(format!("{}{}", parts.last().unwrap(), right));

    view.bufvec[view.cursor_y] = new_lines[0].clone();
    for (idx, line) in new_lines.iter().enumerate().skip(1) {
        view.bufvec.insert(view.cursor_y + idx, line.clone());
    }

    view.cursor_y += parts.len() - 1;
    view.cursor_x = parts.last().unwrap().len();
    view.status.saved = false;
}

fn buf_kill_lines(
    view: &mut View,
    /* start deleting from current cursor (view.cursor_x, view.cursor_y) */
    to: (usize, usize),
) {
    // view.bufvec holds elements by lines. This logic can break if we are to delete a large string
    // with multiple lines. This function correctly handles multi-line-deletion by purging unused
    // lines and merging remaining ones. It may be helpful to referece the 'backspace' logic in key().
    // After deletion, this function will copy the deleted text to view.kill for future paste.
    if view.bufvec.is_empty() {
        view.kill.clear();
        return;
    }

    let mut start_y = view.cursor_y.min(view.bufvec.len().saturating_sub(1));
    let mut start_x = view.cursor_x.min(view.bufvec[start_y].len());

    let mut end_y = to.1.min(view.bufvec.len().saturating_sub(1));
    let mut end_x = to.0.min(view.bufvec[end_y].len());

    if (end_y < start_y) || (end_y == start_y && end_x < start_x) {
        std::mem::swap(&mut start_y, &mut end_y);
        std::mem::swap(&mut start_x, &mut end_x);
    }

    if start_y == end_y && start_x == end_x {
        view.kill.clear();
        return;
    }

    if start_y == end_y {
        let line = &view.bufvec[start_y];
        view.kill = line[start_x..end_x].to_string();
        let new_line = format!("{}{}", &line[..start_x], &line[end_x..]);
        view.bufvec[start_y] = new_line;
    } else {
        let mut killed = String::new();
        killed.push_str(&view.bufvec[start_y][start_x..]);
        killed.push('\n');

        for line in &view.bufvec[start_y + 1..end_y] {
            killed.push_str(line);
            killed.push('\n');
        }
        killed.push_str(&view.bufvec[end_y][..end_x]);

        let prefix = view.bufvec[start_y][..start_x].to_string();
        let suffix = view.bufvec[end_y][end_x..].to_string();

        view.bufvec[start_y] = format!("{}{}", prefix, suffix);

        for _ in 0..(end_y - start_y) {
            view.bufvec.remove(start_y + 1);
        }

        view.kill = killed;
    }

    view.cursor_x = start_x;
    view.cursor_y = start_y;
    view.status.saved = false;
}
