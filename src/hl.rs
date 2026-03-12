use std::sync::LazyLock;
use crate::util;
use crate::config;

pub static FG_SOURCE: LazyLock<String> = LazyLock::new(|| util::hex2ascii!(config::SOURCE));
pub static FG_TYPE: LazyLock<String> = LazyLock::new(|| util::hex2ascii!(config::TYPE));
pub static FG_KEYWORD: LazyLock<String> = LazyLock::new(|| util::hex2ascii!(config::KEYWORD));
pub static FG_CONST: LazyLock<String> = LazyLock::new(|| util::hex2ascii!(config::CONST));
pub static FG_FUNC: LazyLock<String> = LazyLock::new(|| util::hex2ascii!(config::FUNC));
pub static FG_COMMENT: LazyLock<String> = LazyLock::new(|| util::hex2ascii!(config::COMMENT));
pub static BG_DEFAULT: LazyLock<String> = LazyLock::new(|| util::hex2ascii_bg!(config::BACKGROUND));
pub static BG_ACTIVE: LazyLock<String> = LazyLock::new(|| util::hex2ascii_bg!(config::ACTIVE));

pub fn fg_for_scope(scope: &str) -> &'static str {
	if scope.contains("comment") {
		&FG_COMMENT
	} else if scope.contains("entity.name.function")
		|| scope.contains("variable.function")
		|| scope.contains("support.function")
		|| scope.contains("meta.function")
		|| scope.contains("support.macro")
	{
		&FG_FUNC
	} else if scope.contains("storage.type")
		|| scope.contains("support.type")
		|| scope.contains("entity.name.type")
		|| scope.contains("entity.name.class")
		|| scope.contains("entity.name.struct")
		|| scope.contains("entity.name.enum")
		|| scope.contains("entity.name.trait")
	{
		&FG_TYPE
	} else if scope.contains("constant")
		|| scope.contains("string")
		|| scope.contains("character")
		|| scope.contains("numeric")
	{
		&FG_CONST
	} else if scope.contains("keyword")
		|| scope.contains("storage.modifier")
		|| scope.contains("preprocessor")
	{
		&FG_KEYWORD
	} else {
		&FG_SOURCE
	}
}

pub fn render_line_segment(
	screen: &mut String,
	text: &str,
	runs: &[((usize, usize), String)],
	start: usize,
	end: usize,
	bg: &str,
) {
	if start >= end {
		return;
	}

	let width = end - start;
	let mut colors: Vec<&str> = vec![FG_SOURCE.as_str(); width];

	screen.push_str(bg);

	for ((run_start, run_end), scope) in runs {
		let seg_start = (*run_start).max(start);
		let seg_end = run_end.saturating_add(1).min(end);
		if seg_start < seg_end {
			let fg = fg_for_scope(scope);
			for color in &mut colors[(seg_start - start)..(seg_end - start)] {
				*color = fg;
			}
		}
	}

	let mut current_fg = colors[0];
	let mut seg_start = start;
	screen.push_str(current_fg);

	for idx in (start + 1)..end {
		let fg = colors[idx - start];
		if fg != current_fg {
			screen.push_str(&text[seg_start..idx]);
			screen.push_str(fg);
			current_fg = fg;
			seg_start = idx;
		}
	}

	screen.push_str(&text[seg_start..end]);
	screen.push_str(bg);
	screen.push_str(&FG_SOURCE);
}
