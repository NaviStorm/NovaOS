// This file is part of the uutils grep package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::{Config, RegexMode};
use grep_engine::Literal;
use grep_engine::{Pattern, Segment, Selection};
use onig::{RegexOptions, Region, Syntax, SyntaxBehavior, SyntaxOperator};
use onig_sys::{ONIGERR_EMPTY_RANGE_IN_CHAR_CLASS, OnigEncodingUTF8};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use uucore::error::{UResult, USimpleError};
use uucore::show_warning;

static ONIG_NEW_MUTEX: Mutex<()> = Mutex::new(());

pub struct Matcher<'a> {
    config: &'a Config<'a>,
    patterns: Vec<CompiledPattern>,
    perl: Option<crate::perl::Perl>,
    /// One substring searcher per pattern, present only when *every* pattern is
    /// a plain literal that a raw byte search resolves exactly (see
    /// [`plain_literal`]). When set, a caller can decide a line matches by
    /// looking for any of these needles, bypassing the regex engine entirely.
    /// `None` as soon as a single pattern needs real regex evaluation.
    literal_searchers: Option<Vec<Literal>>,
}

impl<'a> Matcher<'a> {
    pub fn compile(config: &'a Config<'a>) -> UResult<Self> {
        // If we can reduce the whole pattern set to literal needles, keep a
        // searcher for each so the driver can take a bulk substring-scan path.
        let needles: Option<Vec<Vec<u8>>> = config
            .patterns
            .iter()
            .map(|p| plain_literal(p, config.ignore_case, config.regex_mode))
            .collect();
        let literal_searchers = needles
            .filter(|n| !n.is_empty())
            .map(|n| n.into_iter().map(Literal::new).collect());

        let mut patterns = Vec::with_capacity(config.patterns.len());
        let mut perl = None;
        for raw in config.patterns {
            if config.regex_mode == RegexMode::Perl {
                let (prefix, suffix): (&[u8], &[u8]) = if config.line_regexp {
                    (br"\A(?:", br")\z")
                } else if config.word_regexp {
                    (br"(?<!\w)(?:", br")(?!\w)")
                } else {
                    (b"", b"")
                };
                let pattern = [prefix, *raw, suffix].concat();
                perl = Some(
                    crate::perl::Perl::compile(&pattern, config.utf8, config.ignore_case)
                        .map_err(|e| USimpleError::new(2, e))?,
                );
            } else if literal_searchers.is_none() {
                patterns.push(CompiledPattern::compile(raw, config)?);
            }
        }

        Ok(Self {
            config,
            patterns,
            perl,
            literal_searchers,
        })
    }

    pub fn select(
        &mut self,
        line: &[u8],
        positions: bool,
    ) -> std::io::Result<Option<Vec<(usize, usize)>>> {
        if !positions
            && !self.config.word_regexp
            && !self.config.line_regexp
            && let Some(needles) = self.literal_searchers()
        {
            let found = grep_engine::leftmost_match(needles, line).is_some();
            return Ok((found != self.config.invert_match).then(Vec::new));
        }
        let Some(perl) = self.perl.as_mut() else {
            let selection = Selection {
                utf8: self.config.utf8,
                word: self.config.word_regexp,
                whole: self.config.line_regexp,
                inverted: self.config.invert_match,
            };
            if let Some(literals) = &self.literal_searchers {
                return Ok(selection
                    .select(literals, line, positions)
                    .unwrap_or_else(|never| match never {}));
            }
            return selection.select(&self.patterns, line, positions);
        };
        let mut spans = Vec::new();
        let mut matched = false;
        let mut offset = 0;
        while offset <= line.len() {
            let Some((start, end)) = perl.find(line, offset)? else {
                break;
            };
            matched = true;
            if !positions || self.config.invert_match {
                break;
            }
            if end > start {
                spans.push((start, end));
            }
            offset = if end > start {
                end
            } else {
                let mut next = start + 1;
                while self.config.utf8 && next < line.len() && (line[next] & 0xc0) == 0x80 {
                    next += 1;
                }
                next
            };
        }
        Ok((matched != self.config.invert_match).then_some(spans))
    }

    /// Per-pattern substring searchers, present only when the pattern set is a
    /// pure set of literals (no regex needed). Used by the searcher to scan a
    /// whole buffer at once instead of testing line by line.
    pub fn literal_searchers(&self) -> Option<&[Literal]> {
        self.literal_searchers.as_deref()
    }
}

/// Return the literal bytes of `pattern` when a raw byte-for-byte substring
/// search is *exactly* equivalent to matching it, otherwise `None`.
///
/// We accept only ASCII, case-sensitive needles. That keeps the byte search in
/// agreement with the regex engine on every possible input, including bytes that
/// are not valid UTF-8: an ASCII byte can never be part of a multi-byte sequence,
/// so its presence is unambiguous. In the regex modes we also require that no
/// byte could ever act as a metacharacter; under `-F` the text is literal as-is.
fn plain_literal(pattern: &[u8], ignore_case: bool, mode: RegexMode) -> Option<Vec<u8>> {
    if ignore_case || pattern.is_empty() || (!pattern.is_ascii() && mode != RegexMode::Fixed) {
        return None;
    }
    // Every byte that carries special meaning in any of our regex syntaxes.
    // A needle without these reads the same as a literal in Basic/Extended/Perl.
    const SPECIAL: &[u8] = b".*[]^$\\+?{}()|";
    let plain = mode == RegexMode::Fixed || !pattern.iter().any(|b| SPECIAL.contains(b));
    plain.then(|| pattern.to_vec())
}

struct CompiledPattern {
    /// Default semantics. It's decently fast and used for searching.
    leftmost: OnigRegex,
    word: bool,
    utf8: bool,
    /// Compiled with `FIND_LONGEST`. If used for a search, it'll search the
    /// entire haystack to find the longest. This makes it unsuitable for searching,
    /// but it's perfect for a second, anchored match pass for POSIX semantics.
    longest_anchored: OnigRegex,
}

impl CompiledPattern {
    fn compile(pattern: &[u8], config: &Config) -> UResult<Self> {
        let mut syntax = *match config.regex_mode {
            RegexMode::Fixed => Syntax::asis(),
            RegexMode::Basic => Syntax::grep(),
            RegexMode::Extended => Syntax::gnu_regex(),
            RegexMode::Perl => unreachable!("PCRE2 owns Perl patterns"),
        };
        if config.regex_mode != RegexMode::Fixed {
            // GNU grep supports `{,n}` as an alias for `{0,n}`.
            syntax.enable_behavior(SyntaxBehavior::SYNTAX_BEHAVIOR_ALLOW_INTERVAL_LOW_ABBREV);
        }
        if matches!(config.regex_mode, RegexMode::Basic | RegexMode::Extended) {
            // GNU grep supports \` and \' as buffer anchors in BRE and ERE.
            syntax.enable_operators(SyntaxOperator::SYNTAX_OPERATOR_ESC_GNU_BUF_ANCHOR);
        }
        if config.regex_mode == RegexMode::Extended {
            // GNU grep -E reads an unmatched `)` as a literal.
            syntax.enable_behavior(SyntaxBehavior::SYNTAX_BEHAVIOR_ALLOW_UNMATCHED_CLOSE_SUBEXP);
        }

        if matches!(config.regex_mode, RegexMode::Basic | RegexMode::Extended)
            && has_confusing_bracket(pattern)
        {
            return Err(USimpleError::new(
                2,
                "character class syntax is [[:space:]], not [:space:]".to_string(),
            ));
        }

        let pattern = if config.regex_mode == RegexMode::Extended
            && let Ok(text) = std::str::from_utf8(pattern)
            && let Some((op, rest)) = strip_leading_repeat_operator(text)
        {
            show_warning!("{op} at start of expression");
            rest.as_bytes()
        } else {
            pattern
        };

        let mut options = RegexOptions::REGEX_OPTION_NONE;
        if config.ignore_case {
            options |= RegexOptions::REGEX_OPTION_IGNORECASE;
        }
        // In GNU grep's Basic/Extended modes, `-z` makes newline ordinary data
        // for `.`, but PCRE keeps its existing non-DOTALL behavior. The GNU
        // `pcre-context` test documents this as current behavior until PCRE2.
        if config.null_data && matches!(config.regex_mode, RegexMode::Basic | RegexMode::Extended) {
            options |= RegexOptions::REGEX_OPTION_MULTILINE;
        }

        fn compile_with(
            pattern: &[u8],
            syntax: &Syntax,
            options: RegexOptions,
            utf8: bool,
        ) -> UResult<OnigRegex> {
            OnigRegex::compile(pattern, syntax, options, utf8).map_err(|err| {
                // A reversed range like `[b-a]` is ONIGERR_EMPTY_RANGE_IN_CHAR_CLASS.
                // GNU grep reports it simply as "Invalid range end" (no pattern
                // echoed), so translate this code to match its diagnostic.
                let message = match err.code {
                    ONIGERR_EMPTY_RANGE_IN_CHAR_CLASS => "Invalid range end".to_string(),
                    onig_sys::ONIGERR_PREMATURE_END_OF_CHAR_CLASS
                    | onig_sys::ONIGERR_END_PATTERN_AT_LEFT_BRACKET => if pattern.ends_with(b"[") {
                        "Invalid regular expression"
                    } else {
                        "Unmatched [, [^, [:, [., or [="
                    }
                    .to_string(),
                    onig_sys::ONIGERR_END_PATTERN_AT_ESCAPE => "Trailing backslash".to_string(),
                    _ => format!(
                        "invalid pattern \"{}\": {}",
                        String::from_utf8_lossy(pattern),
                        err.message
                    ),
                };
                USimpleError::new(2, message)
            })
        }

        let utf8 = config.utf8 && std::str::from_utf8(pattern).is_ok();
        let leftmost = compile_with(pattern, &syntax, options, utf8)?;
        let longest_anchored = compile_with(
            pattern,
            &syntax,
            options | RegexOptions::REGEX_OPTION_FIND_LONGEST,
            utf8,
        )?;
        Ok(Self {
            leftmost,
            word: config.word_regexp && !config.line_regexp,
            utf8,
            longest_anchored,
        })
    }
}

impl Pattern for CompiledPattern {
    type Error = std::io::Error;
    fn find(&self, part: Segment<'_>, from: usize) -> Result<Option<(usize, usize)>, Self::Error> {
        let mut flags = 0;
        if !part.first {
            flags |= onig_sys::ONIG_OPTION_NOTBOL | onig_sys::ONIG_OPTION_NOT_BEGIN_STRING;
        }
        if !part.last {
            flags |= onig_sys::ONIG_OPTION_NOTEOL | onig_sys::ONIG_OPTION_NOT_END_STRING;
        }
        let mut region = Region::new();
        if self
            .leftmost
            .search(part.bytes, from, Some(&mut region), flags)?
            .is_none()
        {
            return Ok(None);
        }
        let (start, end) = region.pos(0).expect("Oniguruma matched group zero");
        self.longest_anchored
            .match_at(part.bytes, start, Some(&mut region), flags)?;
        let mut end = region.pos(0).map_or(end, |(_, e)| e);
        if self.word && !grep_engine::word_boundary(part.bytes, start, end, self.utf8) {
            // GNU -w can choose a shorter alternative at the same start.
            for limit in (start..end).rev() {
                if !grep_engine::word_boundary(part.bytes, start, limit, self.utf8) {
                    continue;
                }
                if self.utf8 && part.bytes.get(limit).is_some_and(|b| b & 0xc0 == 0x80) {
                    continue;
                }
                let flags =
                    flags | onig_sys::ONIG_OPTION_NOTEOL | onig_sys::ONIG_OPTION_NOT_END_STRING;
                let mut shorter = Region::new();
                self.longest_anchored.match_at(
                    &part.bytes[..limit],
                    start,
                    Some(&mut shorter),
                    flags,
                )?;
                if shorter.pos(0) == Some((start, limit)) {
                    end = limit;
                    break;
                }
            }
        }
        Ok(Some((start, end)))
    }
}

struct OnigRegex {
    raw: onig_sys::OnigRegex,
}

// SAFETY: Oniguruma compiled regexes are immutable after construction, and this
// wrapper owns and frees the raw pointer exactly once. This mirrors `onig::Regex`.
unsafe impl Send for OnigRegex {}
// SAFETY: Searches only read the compiled regex. Capture storage is caller-owned
// through `Region`, so sharing the compiled regex across threads is safe.
unsafe impl Sync for OnigRegex {}

impl OnigRegex {
    fn compile(
        pattern: &[u8],
        syntax: &Syntax,
        options: RegexOptions,
        utf8: bool,
    ) -> Result<Self, OnigError> {
        let mut raw = null_mut();
        let mut error = onig_sys::OnigErrorInfo {
            enc: null_mut(),
            par: null_mut(),
            par_end: null_mut(),
        };
        // SAFETY: This reads Oniguruma's process default case-folding bitset.
        let mut case_fold_flag = unsafe { onig_sys::onig_get_default_case_fold_flag() };
        if options.contains(RegexOptions::REGEX_OPTION_IGNORECASE) {
            case_fold_flag &= !onig_sys::INTERNAL_ONIGENC_CASE_FOLD_MULTI_CHAR;
        }

        let encoding = if utf8 {
            &raw mut OnigEncodingUTF8
        } else {
            &raw mut onig_sys::OnigEncodingASCII
        };
        let mut compile_info = onig_sys::OnigCompileInfo {
            num_of_elements: 5,
            pattern_enc: encoding,
            target_enc: encoding,
            syntax: syntax as *const Syntax as *mut Syntax as *mut onig_sys::OnigSyntaxType,
            option: options.bits(),
            case_fold_flag,
        };

        let _guard = ONIG_NEW_MUTEX.lock().unwrap();
        // SAFETY: `pattern` supplies a valid start/end pointer pair for the
        // duration of the call, and `compile_info` uses Oniguruma's built-in
        // UTF-8 encoding plus a syntax value borrowed from the safe wrapper.
        let result = unsafe {
            onig_sys::onig_new_deluxe(
                &mut raw,
                pattern.as_ptr(),
                pattern.as_ptr().add(pattern.len()),
                &mut compile_info,
                &mut error,
            )
        };
        if result == onig_sys::ONIG_NORMAL as i32 {
            Ok(Self { raw })
        } else {
            Err(OnigError::new(result, &error))
        }
    }

    fn search(
        &self,
        line: &[u8],
        offset: usize,
        region: Option<&mut Region>,
        flags: u32,
    ) -> std::io::Result<Option<usize>> {
        debug_assert!(offset <= line.len());
        // SAFETY: `offset` is bounded by `line.len()`, all byte pointers are
        // derived from `line`, and `region_ptr` preserves `onig::Region`'s
        // transparent representation over `OnigRegion`.
        let result = unsafe {
            let start = line.as_ptr().add(offset);
            let end = line.as_ptr().add(line.len());
            onig_sys::onig_search(
                self.raw,
                line.as_ptr(),
                end,
                start,
                end,
                region_ptr(region),
                flags,
            )
        };
        onig_match_result(result)
    }

    fn match_at(
        &self,
        line: &[u8],
        offset: usize,
        region: Option<&mut Region>,
        flags: u32,
    ) -> std::io::Result<Option<usize>> {
        debug_assert!(offset <= line.len());
        // SAFETY: `offset` is bounded by `line.len()`, all byte pointers are
        // derived from `line`, and `region_ptr` preserves `onig::Region`'s
        // transparent representation over `OnigRegion`.
        let result = unsafe {
            let at = line.as_ptr().add(offset);
            onig_sys::onig_match(
                self.raw,
                line.as_ptr(),
                line.as_ptr().add(line.len()),
                at,
                region_ptr(region),
                flags,
            )
        };
        onig_match_result(result)
    }
}

impl Drop for OnigRegex {
    fn drop(&mut self) {
        // SAFETY: `raw` was returned by a successful `onig_new_deluxe` call and
        // is owned by this wrapper.
        unsafe { onig_sys::onig_free(self.raw) }
    }
}

struct OnigError {
    code: i32,
    message: String,
}

impl OnigError {
    fn new(code: i32, info: *const onig_sys::OnigErrorInfo) -> Self {
        Self {
            code,
            message: onig_error_message(code, info),
        }
    }
}

fn region_ptr(region: Option<&mut Region>) -> *mut onig_sys::OnigRegion {
    region.map_or(null_mut(), |r| {
        r as *mut Region as *mut onig_sys::OnigRegion
    })
}

fn onig_match_result(result: i32) -> std::io::Result<Option<usize>> {
    if result >= 0 {
        Ok(Some(result as usize))
    } else if result == onig_sys::ONIG_MISMATCH {
        Ok(None)
    } else {
        Err(std::io::Error::other(onig_error_message(result, null())))
    }
}

fn onig_error_message(code: i32, info: *const onig_sys::OnigErrorInfo) -> String {
    let mut buff = [0; onig_sys::ONIG_MAX_ERROR_MESSAGE_LEN as usize];
    let len = unsafe { onig_sys::onig_error_code_to_str(buff.as_mut_ptr(), code, info) };
    String::from_utf8_lossy(&buff[..len as usize]).into_owned()
}

fn strip_leading_repeat_operator(pattern: &str) -> Option<(&'static str, &str)> {
    match pattern.as_bytes().first()? {
        b'?' => Some(("?", &pattern[1..])),
        b'*' => Some(("*", &pattern[1..])),
        b'+' => Some(("+", &pattern[1..])),
        b'{' => strip_leading_interval_repeat(pattern).map(|rest| ("{...}", rest)),
        _ => None,
    }
}

fn strip_leading_interval_repeat(pattern: &str) -> Option<&str> {
    let close = pattern.as_bytes().iter().position(|&b| b == b'}')?;
    let body = &pattern[1..close];
    let is_interval = !body.is_empty()
        && body.bytes().all(|b| b.is_ascii_digit() || b == b',')
        && body.bytes().any(|b| b.is_ascii_digit());
    is_interval.then_some(&pattern[close + 1..])
}

/// True when `pattern` holds a bracket expression that looks like a misspelled
/// character class, e.g. `[:space:]` instead of `[[:space:]]`. GNU grep rejects
/// those: a bracket whose first and last characters are colons, that holds at
/// least one other character, and that contains no range, class, equivalence
/// class or collating element.
fn has_confusing_bracket(pattern: &[u8]) -> bool {
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'\\' => i += 2,
            b'[' => {
                let (confusing, next) = scan_bracket(pattern, i + 1);
                if confusing {
                    return true;
                }
                i = next;
            }
            _ => i += 1,
        }
    }
    false
}

/// Scan the body of a bracket expression starting at `start` (just past the
/// `[`). Returns whether it is a misspelled character class and the index just
/// past its closing `]`.
fn scan_bracket(pattern: &[u8], start: usize) -> (bool, usize) {
    const FIRST_IS_COLON: u8 = 1;
    const LAST_IS_COLON: u8 = 2;
    const HAS_OTHER: u8 = 4;
    const HAS_RANGE_OR_CLASS: u8 = 8;

    let mut i = start;
    if pattern.get(i) == Some(&b'^') {
        i += 1;
    }
    let body_start = i;
    let mut state = if pattern.get(i) == Some(&b':') {
        FIRST_IS_COLON
    } else {
        0
    };
    while i < pattern.len() {
        let c = pattern[i];
        // A `]` right at the start of the body is an ordinary character.
        if c == b']' && i != body_start {
            return (state == FIRST_IS_COLON | LAST_IS_COLON | HAS_OTHER, i + 1);
        }
        // Only the character just before the closing `]` counts as the last one.
        state &= !LAST_IS_COLON;
        // `[:alpha:]`, `[.a.]` and `[=a=]` inside the bracket.
        if c == b'[' && matches!(pattern.get(i + 1), Some(b':' | b'.' | b'=')) {
            let delimiter = pattern[i + 1];
            if let Some(end) = find_bracket_subexpr_end(pattern, i + 2, delimiter) {
                state |= HAS_RANGE_OR_CLASS;
                i = end;
                continue;
            }
        }
        // `x-y` is a range, but the `-` of `[x-]` is an ordinary character.
        if pattern.get(i + 1) == Some(&b'-')
            && matches!(pattern.get(i + 2), Some(&other) if other != b']')
        {
            state |= HAS_RANGE_OR_CLASS;
            i += 3;
            continue;
        }
        state |= if c == b':' { LAST_IS_COLON } else { HAS_OTHER };
        i += 1;
    }
    // Unterminated bracket: the regex engine reports that on its own.
    (false, pattern.len())
}

/// Index just past the `:]`, `.]` or `=]` closing a `[: [. [=` subexpression
/// whose body starts at `start`.
fn find_bracket_subexpr_end(pattern: &[u8], start: usize, delimiter: u8) -> Option<usize> {
    (start..pattern.len().saturating_sub(1))
        .find(|&i| pattern[i] == delimiter && pattern[i + 1] == b']')
        .map(|i| i + 2)
}

#[cfg(test)]
mod tests {
    use super::{has_confusing_bracket, plain_literal};
    use crate::RegexMode;

    fn lit(p: &str, ic: bool, mode: RegexMode) -> Option<Vec<u8>> {
        plain_literal(p.as_bytes(), ic, mode)
    }

    #[test]
    fn fixed_mode_takes_any_ascii_verbatim() {
        // Under -F every byte is literal, even regex metacharacters.
        assert_eq!(lit("abc", false, RegexMode::Fixed), Some(b"abc".to_vec()));
        assert_eq!(lit("a.*b", false, RegexMode::Fixed), Some(b"a.*b".to_vec()));
        assert_eq!(lit("a+b", false, RegexMode::Fixed), Some(b"a+b".to_vec()));
    }

    #[test]
    fn regex_modes_accept_metacharacter_free_literals() {
        for mode in [RegexMode::Basic, RegexMode::Extended, RegexMode::Perl] {
            assert_eq!(lit("ing", false, mode), Some(b"ing".to_vec()));
            assert_eq!(lit("Hello123", false, mode), Some(b"Hello123".to_vec()));
        }
    }

    #[test]
    fn regex_modes_reject_anything_with_a_metacharacter() {
        for mode in [RegexMode::Basic, RegexMode::Extended, RegexMode::Perl] {
            for p in [
                "a.b", "a*", "[ab]", "^a", "a$", "a\\b", "a+", "a?", "(a)", "a|b", "a{2}",
            ] {
                assert_eq!(lit(p, false, mode), None, "pattern {p:?} in {mode:?}");
            }
        }
    }

    #[test]
    fn rejects_empty_case_insensitive_and_non_ascii() {
        assert_eq!(lit("", false, RegexMode::Fixed), None);
        assert_eq!(lit("abc", true, RegexMode::Fixed), None); // -i
        assert_eq!(lit("abc", true, RegexMode::Basic), None);
        assert_eq!(
            lit("café", false, RegexMode::Fixed),
            Some("café".as_bytes().to_vec())
        ); // non-ASCII
        assert_eq!(lit("naïve", false, RegexMode::Basic), None);
    }

    #[test]
    fn detects_misspelled_character_classes() {
        for p in [
            "[:digit:]",
            "[^:digit:]",
            "q[:punct:]w",
            "[:notaclass:]",
            "[:x:]",
            "ab[:blank:]",
        ] {
            assert!(has_confusing_bracket(p.as_bytes()), "pattern {p:?}");
        }
    }

    #[test]
    fn accepts_bracket_expressions_that_are_not_confusing() {
        for p in [
            "[[:digit:]]",    // the correct spelling
            "[::]",           // no character besides the colons
            "[:digit]",       // does not end with a colon
            "[:digit:qrs]",   // ends with an ordinary character
            "[:dig-it:]",     // holds a range
            "[:x[:digit:]:]", // holds a character class
            "[:x[.,.]:]",     // holds a collating element
            "[:x[=e=]:]",     // holds an equivalence class
            "\\[:digit:]",    // the bracket is escaped
            "[]:digit:]",     // starts with a literal ']'
            "[:digit:",       // unterminated
            "[a-z]+[0-9]",    // no colons at all
        ] {
            assert!(!has_confusing_bracket(p.as_bytes()), "pattern {p:?}");
        }
    }
}
