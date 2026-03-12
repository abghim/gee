macro_rules! hex2ascii {
    ($hx:expr) => {
        format!(
            "\x1b[38;2;{};{};{}m",
            (($hx >> 16) & 0xff) as u64,
            (($hx >> 8) & 0xff) as u64,
            ($hx & 0xff) as u64
        )
    };
}

pub(crate) use hex2ascii;

macro_rules! hex2ascii_bg {
    ($hx:expr) => {
        format!(
            "\x1b[48;2;{};{};{}m",
            (($hx >> 16) & 0xff) as u64,
            (($hx >> 8) & 0xff) as u64,
            ($hx & 0xff) as u64
        )
    };
}

pub(crate) use hex2ascii_bg;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn hextest1() {
		assert_eq!(hex2ascii!(0x123456), "\x1b[38;2;18;52;86m");
	}

	#[test]
	fn hextest_bg() {
		assert_eq!(hex2ascii_bg!(0xabcdef), "\x1b[48;2;171;205;239m");
	}
}
