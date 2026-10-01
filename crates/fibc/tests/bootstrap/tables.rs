//! The vocabulary of the generated inputs (`soup.rs`): tokens that read
//! (`*_OK`) and tokens that do not (`*_BAD`), by kind. The unit tests of
//! `soup.rs` read every entry with the Rust reader to show each list holds
//! what its name says.

pub const SYMBOLS_OK: &[&str] = &[
    "x",
    "foo",
    "foo-bar",
    "a/b",
    "/",
    "x:",
    "->",
    ".",
    "...",
    "_",
    "_1",
    "&",
    "a&b",
    "a#b",
    "λ",
    "é😀",
    "set!",
    "nil?",
    "+",
    "-",
    "--1",
    "-a",
    ".5",
    "+5",
    "a.b",
    "a:b",
    "quote",
    "<=",
    "e\u{301}",
    "a\u{200B}b",
    "nilx",
    "truex",
    "α/β",
    "a-1",
];

pub const SYMBOLS_BAD: &[&str] = &["a//b", "a/", "/a", "a/b/c", "//", "a/b/"];

pub const KEYWORDS_OK: &[&str] = &[
    ":a", ":a/b", ":a:b", ":é", ":x:", ":#", ":1", ":-", ":a.b", ":&", ":_", ":nil", ":/",
];

pub const KEYWORDS_BAD: &[&str] = &[":", "::", "::a", ":a/", ":/a", ":a/b/c", ":a/b/"];

pub const INTS_OK: &[&str] = &[
    "0",
    "1",
    "42",
    "-7",
    "-0",
    "007",
    "0x1F",
    "0xff",
    "0b1010",
    "1_000",
    "1_0_0",
    "1i8",
    "127i8",
    "-128i8",
    "32767i16",
    "2147483647i32",
    "9223372036854775807",
    "-9223372036854775808",
    "0x7fffffffffffffff",
    "-0x8000000000000000i64",
    "0b1111111i8",
    "0xFFi16",
    "-1i64",
];

pub const INTS_BAD: &[&str] = &[
    "0X1F",
    "0b2",
    "0x",
    "0b",
    "-0x",
    "1__0",
    "1_",
    "0x_1",
    "0x1_",
    "128i8",
    "-129i8",
    "255u8",
    "32768i16",
    "2147483648i32",
    "9223372036854775808",
    "-9223372036854775809",
    "0x8000000000000000",
    "0xFFFFFFFFFFFFFFFFFFFF",
    "1i128",
    "1f32",
    "1i",
    "12abc",
    "1.2.3",
    "99999999999999999999999999999999999999999",
    "0b10000000i8",
    "1__0i8",
    "0x1G",
];

pub const FLOATS_OK: &[&str] = &[
    "1.5",
    "-0.5",
    "0.0",
    "-0.0",
    "3.14",
    "1e9",
    "1E9",
    "2.5e-3",
    "1e+5",
    "1.5e3",
    "1_0.5",
    "1e1_0",
    "2.5f32",
    "2.5f64",
    "1e10f32",
    "3.4028235e38f32",
    "0.1f32",
    "16777217.0f32",
    "5e-324",
    "1.7976931348623157e308",
    "123456789012345678901234567890.5",
    "0.000001",
    "1e21",
    "1e22",
    "1e-7",
    "100000.0",
    "0.30000000000000004",
    "1e-45f32",
    "1e-400",
    "1e-46f32",
];

pub const FLOATS_BAD: &[&str] = &[
    "1e",
    "1e+",
    "5.",
    "1.e5",
    "1._5",
    "1.5_",
    "2.5f16",
    "1.5i32",
    "1e400",
    "-1e400f64",
    "3.4028236e38f32",
    "1e39f32",
    "1e_5",
    "1.5f",
    "1.5e",
    "0x1.5",
];

pub const STRINGS_OK: &[&str] = &[
    "\"\"",
    "\"abc\"",
    "\"a b\"",
    "\"é😀\"",
    "\"a\nb\"",
    "\"a\r\nb\"",
    "\"\\n\\t\\r\\0\\\\\\\"\"",
    "\"\\x41\"",
    "\"\\x7F\"",
    "\"\\x00\"",
    "\"\\u{41}\"",
    "\"\\u{0}\"",
    "\"\\u{1F600}\"",
    "\"\\u{10FFFF}\"",
    "\"; not a comment\"",
    "\"((\"",
    "\"\u{A0}\u{85}\u{FEFF}\0\"",
];

pub const STRINGS_BAD: &[&str] = &[
    "\"\\x80\"",
    "\"\\xZZ\"",
    "\"\\x4\"",
    "\"\\u{110000}\"",
    "\"\\u{D800}\"",
    "\"\\u{}\"",
    "\"\\u{1234567}\"",
    "\"\\u0041\"",
    "\"\\u{12\"",
    "\"\\q\"",
    "\"\\'\"",
    "\"unterminated",
    "\"a\\\"",
    "\"\\",
];

pub const CHARS_OK: &[&str] = &[
    "\\a",
    "\\Z",
    "\\\\",
    "\\(",
    "\\\"",
    "\\;",
    "\\newline",
    "\\space",
    "\\tab",
    "\\return",
    "\\u{41}",
    "\\u{1F600}",
    "\\é",
    "\\😀",
    "\\@",
    "\\,",
    "\\&",
    "\\#",
    "\\'",
    "\\`",
    "\\{",
    "\\]",
    "\\u",
    "\\x",
];

pub const CHARS_BAD: &[&str] = &[
    "\\ ",
    "\\nope",
    "\\u{D800}",
    "\\u{}",
    "\\u0041",
    "\\ab",
    "\\",
    "\\newlines",
    "\\\u{A0}",
    "\\\0",
    "\\\n",
    "\\\r\n",
    "\\u{110000}",
    "\\u{12",
];

/// Integers that are well formed but do not fit their width.
pub const INT_RANGE_BAD: &[&str] = &[
    "128i8",
    "-129i8",
    "32768i16",
    "2147483648i32",
    "9223372036854775808",
    "-9223372036854775809",
    "0x8000000000000000",
    "0xFFFFFFFFFFFFFFFFFFFF",
    "99999999999999999999999999999999999999999",
    "0b10000000i8",
];

/// Floats that are well formed but too large for their width.
pub const FLOAT_RANGE_BAD: &[&str] = &[
    "1e400",
    "-1e400f64",
    "3.4028236e38f32",
    "1e39f32",
    "1e309",
    "-3.5e38f32",
];

pub const LITERALS_OK: &[&str] = &["true", "false", "nil", "()", "[]", "{}"];

pub const ODD_BAD: &[&str] = &[
    "\u{A0}", "\u{85}", "\u{FEFF}", "\0", "\u{2028}", "\u{B}", "\u{C}", "\u{7F}", "\u{1}",
    "\u{3000}",
];

pub const MISC_BAD: &[&str] = &[
    "#", "#x", "#!", "#(", "#{", "#\"", "##", "#é", ")", "]", "}", "#_", "@ ", ",@ ", "& (", "{1}",
    "'", "@", ",@",
];

pub const COMMENTS: &[&str] = &[
    "; a comment",
    ";",
    ";; é😀 x",
    "; (unclosed \"string",
    "; \u{A0} \0",
];

/// The read errors that one short snippet provokes, a group for each kind
/// (every kind but `TooDeep`, which `soup::deep` makes), in the order of
/// [`TARGETED_KINDS`]. `soup::targeted` puts one after a clean input, so
/// each is the first error the reader meets.
pub const TARGETED: &[&[&str]] = &[
    &["\"abc", "(a \"b\n", "\"a\\\"", "\""],
    &["\"\\q\"", "\"a\\xZZ\"", "\"\\x80\"", "\"\\x4\"", "\"\\'\""],
    &[
        "\"\\u41\"",
        "\"\\u{110000}\"",
        "\"\\u{}\"",
        "\"\\u{D800}\"",
        "\"\\u{12\"",
        "\"\\u{1234567}\"",
        "\\u{D800}",
    ],
    &["\\nope", "\\ ", "(a \\ )", "\\newlines", "\\u0041", "\\ab"],
    &[
        "x\u{A0}y",
        "(a \u{85})",
        "x\0y",
        "a \u{2028} b",
        "\\\u{A0}",
        "(a \u{FEFF})",
    ],
    &[
        "1e", "12abc", "1__0", "0x", "0b2", "1.5i32", "255u8", "5.", "1.e5",
    ],
    &[
        "300i8",
        "9223372036854775808",
        "32768i16",
        "-129i8",
        "0x8000000000000000",
    ],
    &["1e400", "3.4028236e38f32", "-1e400f64", "1e39f32"],
    &["a//b", "a/", "/a", "x/y/z", "//"],
    &[":", "::a", ":a/", ":/a", ":a/b/c"],
    &["#", "#x", "(a #", "#(", "##"],
    &["'", "(a ')", "@ x", ",@", "(a @)", "`"],
    &["#_", "(a #_)", "[#_ ]", "#_ #_ x", "(#_"],
    &["&1", "&(a)", "&\"s\"", "&:k", "&'x"],
    &["(", "[a", "{:a 1", "(a (b", "( \"s\" "],
    &[")", "]", "}", "(a))", "x ]"],
    &["(a ]", "[a )", "{:a 1 )", "(\n [a\n )"],
    &["{a}", "{:a 1 :b}", "{1 2 3}", "[{x}]", "{:a {:b}}"],
];

/// The kind of read error each group of [`TARGETED`] provokes.
pub const TARGETED_KINDS: [&str; 18] = [
    "UnterminatedString",
    "BadEscape",
    "BadUnicodeEscape",
    "BadCharLiteral",
    "InvalidCharacter",
    "InvalidNumber",
    "IntegerOutOfRange",
    "FloatOutOfRange",
    "InvalidSymbol",
    "InvalidKeyword",
    "UnknownDispatch",
    "PrefixWithoutForm",
    "DiscardWithoutForm",
    "InOutNotSymbol",
    "Unclosed",
    "UnexpectedClose",
    "MismatchedClose",
    "OddMapEntries",
];

/// The three kinds of delimiter: `(`, `[`, `{` and what closes each.
pub const OPENERS: [char; 3] = ['(', '[', '{'];
pub const CLOSERS: [char; 3] = [')', ']', '}'];
