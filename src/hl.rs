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
		|| scope.contains("support.function")
		|| scope.contains("meta.function")
		|| scope.contains("support.macro")
		|| scope.contains("entity.name.macro")
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

pub fn scope_at<'a>(runs: &'a [((usize, usize), String)], cursor: usize) -> Option<&'a str> {
	for ((start, end), scope) in runs.iter().rev() {
		if *start <= cursor && cursor <= *end {
			return Some(scope.as_str());
		}
	}

	None
}

pub fn render_line_segment(screen: &mut String, text: &str, runs: &[((usize, usize), String)], start: usize, end: usize, selected: bool) {
	if start >= end {
		return;
	}

	let bg = if selected { &*BG_ACTIVE } else { &*BG_DEFAULT };
	let mut current_fg = "";

	screen.push_str(bg);

	for idx in start..end {
		let fg = scope_at(runs, idx).map(fg_for_scope).unwrap_or(&FG_SOURCE);
		if fg != current_fg {
			screen.push_str(fg);
			current_fg = fg;
		}
		screen.push(text.as_bytes()[idx] as char);
	}

	screen.push_str(&BG_DEFAULT);
	screen.push_str(&FG_SOURCE);
}
