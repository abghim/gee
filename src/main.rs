#![allow(dead_code)]
#![allow(unused)]

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Error, Write};
use onig::Regex;
use termion::screen::AlternateScreen;
use termion::screen::IntoAlternateScreen;
use termion::{event::Key, input::TermRead, raw::IntoRawMode};

mod stack;
mod syntax;
mod util;

mod config;

use crate::util::*;

use crate::stack::*;
use crate::syntax::{
	get_context, get_syntax_info, syntax_id_for_filename, Action, ContextInfo, ContextReference,
	Rule, _Match, syntax_main_and_prototype,
};

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
	pub hlcache: &'a mut Vec<usize>,
	pub line_hl: &'a mut Vec<Vec<((usize, usize), String)>>,
	pub regex_cache: &'a mut HashMap<&'static str, Regex>,
	pub stack: &'a mut Stack,
	pub syntax: Option<u16>,
	pub recompute: usize,
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

/*
 * todo: (1) in main.py expand all variables [v] done
 * 		(2) add prototype auto-inclusion [v] done
 */

#[cfg(test)]
mod tests {
	use super::*;
}

impl<'a> View<'a> {
	fn trueloc(self: &Self) -> (usize, usize) {
		(self.cursor_x, self.cursor_y)
	}

	fn applicable(&self, context: &ContextReference) -> Vec<_Match> {
		/* this avoids duplicates */
		let mut seen_prototypes = HashSet::<ContextReference>::new();
		self.applicable_inner(context, &mut seen_prototypes)
	}

	fn applicable_inner(
		&self,
		context: &ContextReference,
		seen_prototypes: &mut HashSet<ContextReference>,
	) -> Vec<_Match> {
		let info: ContextInfo = get_context(*context);
		let mut result = Vec::<_Match>::new();
		if info.meta_include_prototype {
			if let Some(prototype) = syntax_main_and_prototype(context.0).1 {
				let proto_ref = ContextReference(context.0, prototype);
				if context.1 != prototype && seen_prototypes.insert(proto_ref) {
					result.append(&mut self.applicable_inner(&proto_ref, seen_prototypes));
				}
			}
		}

		for rule in info.rules.iter() {
			match rule {
				Rule::Include(c) => {
					result.append(&mut self.applicable_inner(c, seen_prototypes));
				}

				Rule::Match(m) => {
					result.push(m.clone());
				}
			}
		}

		result
	}

	fn highlight_line(
		self: &mut Self,
		line: usize,
		begin_frame: usize,
	) -> (Vec<((usize, usize), String)>, usize /* out_frame */) {
		use onig::{Region, SearchOptions};
		if let Some(g) = self.syntax {
			let real_line = &self.bufvec[line];
			let parse_line = format!("{real_line}\n");
			let real_len = real_line.len();
			let mut out_frame = begin_frame;
			let mut rules = self.applicable(&self.stack.top(begin_frame).unwrap());
			let mut context = get_context(self.stack.top(begin_frame).unwrap());
			let mut hl: Vec<((usize, usize), String)> = Vec::new();
			let mut cursor = 0;
			'outer: while (cursor < real_len) {
				let mut matched = false;

				rules = self.applicable(&self.stack.top(out_frame).unwrap());

				context = get_context(self.stack.top(out_frame).unwrap());
				'inner: for rule in rules.iter() {
					let regex = self
						.regex_cache
						.entry(rule.pattern)
						.or_insert_with(|| Regex::new(rule.pattern).unwrap());
					let mut region = Region::new();
					if let Some(l) = regex.match_with_options(
						&parse_line,
						cursor,
						SearchOptions::SEARCH_OPTION_NONE,
						Some(&mut region),
					) {
						let len = l;
						matched = true;
						if len > 0 {
							if let Some(scope) = rule.scope {
								if scope.len() == 1 {
									hl.push(((cursor, cursor + len - 1), scope[0].to_string()));
								} else {
									for (i, group) in scope.iter().enumerate().skip(1) {
										if let Some((a, b)) = region.pos(i) {
											if a < real_len {
												hl.push((
													(a, b.min(real_len.saturating_sub(1))),
													group.to_string(),
												));
											}
										} /* if a group is NOT found, don't do anything */
									}
								}
							} else {
								let is_scope_boundary = matches!(
									rule.action,
									Some(Action::Pop) | Some(Action::Set(_)) | Some(Action::Push(_))
								);
								if let Some(meta_s) = context.meta_scope {
									hl.push(((cursor, cursor + len - 1), meta_s.to_string()));
								} else if !is_scope_boundary {
									if let Some(meta_c) = context.meta_content_scope {
										hl.push(((cursor, cursor + len - 1), meta_c.to_string()));
									} else {
										hl.push((
											(cursor, cursor + len - 1),
											get_syntax_info(g).scope.to_string(),
										));
									}
								} else {
									hl.push((
										(cursor, cursor + len - 1),
										get_syntax_info(g).scope.to_string(),
									));
								}
							}
						}

						let frame_before = out_frame;

						if let Some(action) = rule.action {
							match action {
								Action::Pop => out_frame = self.stack.pop(out_frame),

								Action::Set(crefs) => {
									out_frame = self.stack.pop(out_frame);
									for cref in crefs.iter() {
										out_frame = self.stack.push(*cref, out_frame);
									}
								}

								Action::Push(crefs) => {
									for cref in crefs.iter() {
										out_frame = self.stack.push(*cref, out_frame);
									}
								}
							}
						}

						if len == 0 {
							if out_frame == frame_before {
								cursor += 1;
							}
						} else {
							cursor += len;
						}

						break 'inner;
					}
				}
				if !matched {
					if let Some(meta_s) = context.meta_scope {
						hl.push(((cursor, cursor), meta_s.to_string()));
					} else if let Some(meta_c) = context.meta_content_scope {
						hl.push(((cursor, cursor), meta_c.to_string()));
					} else {
						/* nothing at all? use default syntax scope!! */
						hl.push(((cursor, cursor), get_syntax_info(g).scope.to_string()));
					}
					cursor += 1;
				}
			}

			let mut eol_steps = 0usize;
			while eol_steps < 32 {
				let mut changed = false;
				rules = self.applicable(&self.stack.top(out_frame).unwrap());

				for rule in rules.iter() {
					let regex = self
						.regex_cache
						.entry(rule.pattern)
						.or_insert_with(|| Regex::new(rule.pattern).unwrap());
					let mut region = Region::new();
					if let Some(len) = regex.match_with_options(
						&parse_line,
						real_len,
						SearchOptions::SEARCH_OPTION_NONE,
						Some(&mut region),
					) {
						let frame_before = out_frame;
						if let Some(action) = rule.action {
							match action {
								Action::Pop => out_frame = self.stack.pop(out_frame),
								Action::Set(crefs) => {
									out_frame = self.stack.pop(out_frame);
									for cref in crefs.iter() {
										out_frame = self.stack.push(*cref, out_frame);
									}
								}
								Action::Push(crefs) => {
									for cref in crefs.iter() {
										out_frame = self.stack.push(*cref, out_frame);
									}
								}
							}
						}

						if out_frame != frame_before {
							changed = true;
							break;
						}
					}
				}

				if !changed {
					break;
				}
				eol_steps += 1;
			}
			(hl, out_frame) /* moving hl */
		} else {
			panic!("Syntax highlighting function called, but no syntax found");
		}
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

mod hl;
use hl::*;

fn main() -> io::Result<()> {
	let pathstr: String = match std::env::args().nth(1) {
		Some(x) => x,
		None => "Untitled".to_string(),
	};

	let mut working = match OpenOptions::new().read(true).write(true).open(&pathstr) {
		Ok(f) => f,
		Err(_) => File::create_new(&pathstr).expect("File creation error"),
	};

	let filetypeid: Option<u16> = syntax_id_for_filename(&pathstr);

	let buffered = BufReader::new(&working);
	let mut buflines = Vec::<String>::new();

	for line in buffered.lines() {
		let ln = line.unwrap();
		buflines.push(ln.replace('\t', "    "));
	}

	let bufl = buflines.clone();

	let termsize::Size { rows, cols } = termsize::get().unwrap();
	let mut stack = Stack::new();
	let mut hlcache: Vec<usize> = vec![stack.empty(); buflines.len()];
	let mut line_hl: Vec<Vec<((usize, usize), String)>> = vec![Vec::new(); buflines.len().max(1)];
	let mut regex_cache: HashMap<&'static str, Regex> = HashMap::new();

	let mut screen: View = View {
		working_col: 0,
		bufvec: &mut buflines,
		hlcache: &mut hlcache,
		line_hl: &mut line_hl,
		regex_cache: &mut regex_cache,
		stack: &mut stack,
		syntax: filetypeid,
		recompute: 0,
		cursor_x: 0,
		cursor_y: 0,
		offset: 0,
		mark: (0, 0),
		terminal_w: (cols as usize),
		terminal_h: (rows as usize),
		offcol: 0,
		endline: format!(
			"Loaded {}L of {}",
			bufl.len(),
			match filetypeid {
				Some(x) => get_syntax_info(x).name.to_string(),
				None => "Plaintext".to_string(),
			}
		),
		kill: "".to_string(),
		status: Status {
			saved: true,
			quit: false,
			ctrlx: false,
			save: false,
			forcequit: false,
			selecting: false,
		},
	};

	clamp(&mut screen);
	let initial_scope = reparse_dirty(&mut screen);
	let initial_status = screen.endline.clone();
	update_scope_status(&mut screen, Some(initial_status), initial_scope);

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
		let current_scope = reparse_dirty(&mut screen);

		let mut status_message = if screen.status.selecting {
			Some("Selecting".to_string())
		} else {
			None
		};

		if screen.status.save {
			use std::io::{Seek, SeekFrom};

			working.seek(SeekFrom::Start(0))?;
			working.set_len(0)?;
			working
				.write_all(screen.bufvec.join("\n").as_bytes())
				.expect("no write");
			working.flush().expect("Error flushing");

			screen.status.saved = true;
			status_message = Some(format!("Wrote to {}", &pathstr));
			screen.status.save = false;
		}

		if screen.status.quit {
			if screen.status.saved || screen.status.forcequit {
				break;
			} else {
				status_message = Some(format!("{} not saved", &pathstr));
				screen.status.quit = false;
			}
		}
		/* show in status line */
		update_scope_status(&mut screen, status_message, current_scope);
		clamp(&mut screen);
		frame(&mut screen_out, &screen);
	}

	Ok(())
}


fn frame<W: Write>(out: &mut W, view: &View) {
	let mut screen: String = String::with_capacity(
		view.terminal_w
			.saturating_mul(view.terminal_h)
			.saturating_mul(4)
			.saturating_add(256),
	);
	let blank_row = " ".repeat(view.terminal_w);

	screen.push_str(CURSOR_HIDE);
	screen.push_str(&goto(1, 1));

	/* using config, push default fg & bg */
	screen.push_str(&BG_DEFAULT);
	screen.push_str(&FG_SOURCE);



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
		let row_bg = if i == view.cursor_y { &*BG_ACTIVE } else { &*BG_DEFAULT };

		screen.push_str(&goto((n + 1) as u16, 1));
		screen.push_str(row_bg);
		screen.push_str(&FG_SOURCE);

		if i >= view.bufvec.len() {
			screen.push_str(&blank_row);
			continue;
		}

		let line = match view.bufvec.get(i) {
			Some(x) => x,
			None => view.bufvec.last().unwrap(),
		};
		let runs = view.line_hl.get(i).map(|v| v.as_slice()).unwrap_or(&[]);

		let start = view.offcol.min(line.len());
		let end = (view.offcol + view.terminal_w).min(line.len());
		let visible_width = end.saturating_sub(start);

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
				render_line_segment(&mut screen, line, runs, start, vis_sel_start, row_bg);
				screen.push_str(STYLE_INVERT_ON);
				render_line_segment(&mut screen, line, runs, vis_sel_start, vis_sel_end, row_bg);
				screen.push_str(STYLE_INVERT_OFF);
				screen.push_str(row_bg);
				screen.push_str(&FG_SOURCE);
				render_line_segment(&mut screen, line, runs, vis_sel_end, end, row_bg);
				if visible_width < view.terminal_w {
					screen.push_str(row_bg);
					screen.push_str(&blank_row[..view.terminal_w - visible_width]);
				}
				continue;
			}
		}

		render_line_segment(&mut screen, line, runs, start, end, row_bg);
		if visible_width < view.terminal_w {
			screen.push_str(row_bg);
			screen.push_str(&blank_row[..view.terminal_w - visible_width]);
		}
	}

	screen.push_str(&goto(view.terminal_h as u16, 1));
	screen.push_str(&BG_DEFAULT);
	screen.push_str(&FG_SOURCE);
	screen.push_str(&view.endline[..view.endline.len().min(view.terminal_w)]);
	if view.endline.len() < view.terminal_w {
		screen.push_str(&blank_row[..view.terminal_w - view.endline.len()]);
	}
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
		view.hlcache.clear();
		view.hlcache.push(view.stack.empty());
		view.recompute = 0;
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
					mark_recompute(view, view.cursor_y);
				} else if view.cursor_y > 0 {
					let remove_idx = view.cursor_y;
					let cur = view.bufvec.remove(view.cursor_y);
					view.cursor_y -= 1;
					view.cursor_x = view.bufvec[view.cursor_y].len();
					view.bufvec[view.cursor_y].push_str(&cur);
					remove_cache_lines(view, remove_idx, 1);
					mark_recompute(view, view.cursor_y);
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
					mark_recompute(view, view.cursor_y);
				} else if view.cursor_y + 1 < view.bufvec.len() {
					let remove_idx = view.cursor_y + 1;
					let next = view.bufvec.remove(remove_idx);
					view.bufvec[view.cursor_y].push_str(&next);
					remove_cache_lines(view, remove_idx, 1);
					mark_recompute(view, view.cursor_y);
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
				let start_y = view.cursor_y;
				let cur_line = view.bufvec[view.cursor_y].clone();
				let (left, right) = cur_line.split_at(view.cursor_x);
				view.bufvec[view.cursor_y] = left.to_string();
				view.bufvec.insert(view.cursor_y + 1, right.to_string());
				insert_cache_lines(view, start_y + 1, 1);
				view.cursor_y += 1;

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
					insert_cache_lines(view, view.cursor_y + 1, 1);
					mark_recompute(view, start_y);

					view.status.saved = false;
					view.status.selecting = false;
					return;
				}

				view.bufvec[view.cursor_y].insert_str(0, &" ".repeat(base));
				view.cursor_x = base;
				mark_recompute(view, start_y);

				view.status.saved = false;
				view.status.selecting = false;
			}

			Key::Char('\t') => {
				for _ in 0..4 {
					view.bufvec[view.cursor_y].insert(view.cursor_x, ' ');
					view.cursor_x += 1;
				}
				mark_recompute(view, view.cursor_y);
			}

			Key::Char('{') => {
				view.bufvec[view.cursor_y].insert_str(view.cursor_x, "{}");
				view.cursor_x += 1;
				view.status.selecting = false;
				view.status.saved = false;
				mark_recompute(view, view.cursor_y);
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
				mark_recompute(view, view.cursor_y);
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

fn mark_recompute(view: &mut View, line: usize) {
	if view.recompute > line {
		view.recompute = line;
	}
}

fn insert_cache_lines(view: &mut View, at: usize, count: usize) {
	if count == 0 {
		return;
	}
	let fill = view.stack.empty();
	let runs_fill = Vec::new();
	let at = at.min(view.hlcache.len());
	view.hlcache
		.splice(at..at, std::iter::repeat(fill).take(count));
	let at_runs = at.min(view.line_hl.len());
	view.line_hl
		.splice(at_runs..at_runs, std::iter::repeat(runs_fill).take(count));
}

fn remove_cache_lines(view: &mut View, at: usize, count: usize) {
	if count == 0 || view.hlcache.is_empty() {
		return;
	}
	let start = at.min(view.hlcache.len());
	let end = (start + count).min(view.hlcache.len());
	if start < end {
		view.hlcache.drain(start..end);
	}
	let start_runs = at.min(view.line_hl.len());
	let end_runs = (start_runs + count).min(view.line_hl.len());
	if start_runs < end_runs {
		view.line_hl.drain(start_runs..end_runs);
	}
}

fn main_frame(view: &mut View) -> usize {
	match view.syntax {
		Some(k) => view.stack.push(
			ContextReference(k, syntax_main_and_prototype(k).0),
			view.stack.empty(),
		),
		None => view.stack.empty(),
	}
}

fn default_scope(view: &View) -> String {
	view.syntax
		.map(|id| get_syntax_info(id).scope.to_string())
		.unwrap_or_else(|| "text.plain".to_string())
}

fn scope_from_runs(
	scopes: &[((usize, usize), String)],
	cursor: usize,
	default_scope: String,
) -> String {
	for ((start, end), scope) in scopes.iter().rev() {
		if *start <= cursor && cursor <= *end {
			return scope.clone();
		}
	}

	default_scope
}

fn ensure_cache(view: &mut View) {
	let target_len = view.bufvec.len().max(1);
	let fill = view.stack.empty();
	if view.hlcache.len() < target_len {
		view.hlcache
			.extend(std::iter::repeat(fill).take(target_len - view.hlcache.len()));
	} else if view.hlcache.len() > target_len {
		view.hlcache.truncate(target_len);
	}

	if view.line_hl.len() < target_len {
		view.line_hl
			.extend(std::iter::repeat_with(Vec::new).take(target_len - view.line_hl.len()));
	} else if view.line_hl.len() > target_len {
		view.line_hl.truncate(target_len);
	}

	if !view.hlcache.is_empty() {
		let main = main_frame(view);
		view.hlcache[0] = main;
	}
}

fn reparse_dirty(view: &mut View) -> String {
	if view.bufvec.is_empty() {
		view.hlcache.clear();
		view.recompute = 0;
		return "no scope".to_string();
	}

	if view.syntax.is_none() {
		ensure_cache(view);
		let line = view.cursor_y.min(view.bufvec.len().saturating_sub(1));
		if let Some(runs) = view.line_hl.get_mut(line) {
			runs.clear();
		}
		view.recompute = line.saturating_add(1);
		return default_scope(view);
	}

	ensure_cache(view);
	let main = main_frame(view);
	view.hlcache[0] = main;
	let cursor_line = view.cursor_y.min(view.bufvec.len() - 1);
	let margin = 8usize;
	let text_rows = view.terminal_h.saturating_sub(1);
	let visible_end = (view.offset + text_rows + margin).min(view.bufvec.len()).saturating_sub(1);
	let target_end = visible_end.max(cursor_line);
	let mut cursor_scope = default_scope(view);

	if view.recompute > target_end {
		let begin_frame = if cursor_line == 0 {
			main
		} else {
			view.hlcache[cursor_line]
		};
		let (scopes, _) = view.highlight_line(cursor_line, begin_frame);
		let cursor = if view.bufvec[cursor_line].is_empty() {
			0
		} else {
			view.cursor_x.min(view.bufvec[cursor_line].len().saturating_sub(1))
		};
		let scope = scope_from_runs(&scopes, cursor, cursor_scope);
		if let Some(line_runs) = view.line_hl.get_mut(cursor_line) {
			*line_runs = scopes;
		}
		return scope;
	}

	let start = view.recompute.min(view.bufvec.len() - 1);
	for line in start..=target_end {
		let begin_frame = if line == 0 { main } else { view.hlcache[line] };
		let (scopes, out_frame) = view.highlight_line(line, begin_frame);
		if line == cursor_line {
			let cursor = if view.bufvec[line].is_empty() {
				0
			} else {
				view.cursor_x.min(view.bufvec[line].len().saturating_sub(1))
			};
			cursor_scope = scope_from_runs(&scopes, cursor, cursor_scope);
		}
		if let Some(line_runs) = view.line_hl.get_mut(line) {
			*line_runs = scopes;
		}
		if line + 1 >= view.bufvec.len() {
			break;
		}

		view.hlcache[line + 1] = out_frame;
	}

	view.recompute = target_end.saturating_add(1);
	cursor_scope
}

fn scope_under_cursor(view: &mut View) -> String {
	if view.bufvec.is_empty() {
		return "no scope".to_string();
	}

	reparse_dirty(view);

	let line = view.cursor_y.min(view.bufvec.len() - 1);
	let syntax_scope = view
		.syntax
		.map(|id| get_syntax_info(id).scope.to_string())
		.unwrap_or_else(|| "text.plain".to_string());

	if view.syntax.is_none() {
		return syntax_scope;
	}

	let line_len = view.bufvec[line].len();
	if line_len == 0 {
		return syntax_scope;
	}

	let begin_frame = if line == 0 {
		main_frame(view)
	} else {
		view.hlcache[line]
	};
	let (scopes, _) = view.highlight_line(line, begin_frame);
	let cursor = view.cursor_x.min(line_len.saturating_sub(1));

	scope_from_runs(&scopes, cursor, syntax_scope)
}

fn update_scope_status(view: &mut View, prefix: Option<String>, scope: String) {
	view.endline = match prefix {
		Some(msg) if !msg.is_empty() => format!("{msg} | {scope}"),
		_ => scope,
	};
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
		if view.hlcache.is_empty() {
			view.hlcache.push(view.stack.empty());
		}
	}

	if view.cursor_y >= view.bufvec.len() {
		view.cursor_y = view.bufvec.len() - 1;
	}
	view.cursor_x = view.cursor_x.min(view.bufvec[view.cursor_y].len());

	let start_y = view.cursor_y;
	let parts: Vec<&str> = insert.split('\n').collect();
	if parts.len() == 1 {
		view.bufvec[view.cursor_y].insert_str(view.cursor_x, &insert);
		view.cursor_x += insert.len();
		mark_recompute(view, start_y);
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
	insert_cache_lines(view, start_y + 1, parts.len() - 1);

	view.cursor_y += parts.len() - 1;
	view.cursor_x = parts.last().unwrap().len();
	mark_recompute(view, start_y);
	view.status.saved = false;
}

fn buf_kill_lines(
	view: &mut View,
	/* start deleting from current cursor (view.cursor_x, view.cursor_y) */
	to: (usize, usize),) {
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
		mark_recompute(view, start_y);
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
		remove_cache_lines(view, start_y + 1, end_y - start_y);
		mark_recompute(view, start_y);

		view.kill = killed;
	}

	view.cursor_x = start_x;
	view.cursor_y = start_y;
	view.status.saved = false;
}
