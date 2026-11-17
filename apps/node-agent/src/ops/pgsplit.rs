//! Cuts a PostgreSQL dump into statements and `COPY` data (T07.12), the way `pg_restore`'s
//! plain-text reader and `psql` do, but without psql: nothing in a dump is ever run by anything
//! but the database server, as the temporary account of one import.
//!
//! Statements end at a `;` that is not inside a string (`'…'`, and `E'…'` with backslash
//! escapes), a quoted name (`"…"`), a dollar-quoted body (`$tag$…$tag$`) or a comment (`-- …`
//! and `/* … */`, which nests). A statement that is `COPY … FROM stdin` is followed by the
//! rows, up to a line that is exactly `\.`; they are handed out as raw bytes. The only
//! backslash lines accepted outside a `COPY` are `\restrict` and `\unrestrict` (what
//! `pg_dump` writes around a dump since 2025); every other `psql` meta-command (`\connect`,
//! `\!`, …) is an error, so a dump cannot ask for anything the server would not do itself.
//! A statement larger than the limit is an error, never run.

use std::collections::VecDeque;

/// Why reading stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SplitError {
    /// One statement (or one row of `COPY` data) is longer than the limit.
    #[error("a statement is larger than {0} bytes")]
    TooLarge(usize),
    /// The dump ends in the middle of a string, a comment or the rows of a `COPY`.
    #[error("the dump ends inside a string, a comment or a COPY")]
    Unterminated,
    /// Not text the server could read.
    #[error("the dump is not valid UTF-8 text")]
    NotText,
    /// A `psql` meta-command, which is not supported.
    #[error("the dump uses the psql command \\{0}, which is not supported")]
    MetaCommand(String),
}

/// What the reader hands out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A statement, without its `;`.
    Statement(String),
    /// A `COPY … FROM stdin` statement, without its `;`; its rows follow.
    CopyStart(String),
    /// Some rows, with their line ends.
    CopyData(Vec<u8>),
    /// The line `\.`: the rows are over.
    CopyEnd,
}

#[derive(Clone, PartialEq, Eq)]
enum State {
    Normal,
    /// A string; whether backslash escapes the next byte (`E'…'`).
    Single(bool),
    Double,
    Line,
    /// A block comment, how deep.
    Block(u32),
    /// A dollar-quoted body, with its tag (without the dollars).
    Dollar(Vec<u8>),
    /// The rows of a `COPY`.
    Copy,
}

/// The reader. Feed it bytes with [`PgSplitter::push`], take items with
/// [`PgSplitter::next_item`], and call [`PgSplitter::finish`] at the end.
pub struct PgSplitter {
    limit: usize,
    input: Vec<u8>,
    at: usize,
    eof: bool,
    state: State,
    /// The statement being read.
    stmt: Vec<u8>,
    /// Whether the statement has any byte that is not white space or a comment yet.
    code: bool,
    /// Rows of a `COPY` waiting to be handed out.
    rows: Vec<u8>,
    /// The rest of the line the `COPY` statement ended on is not data: skip it.
    skip_line: bool,
    out: VecDeque<Item>,
}

/// Rows are handed out in pieces of about this size.
const ROWS_PIECE: usize = 1 << 20;

impl PgSplitter {
    /// A reader that refuses statements over `limit` bytes.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            input: Vec::new(),
            at: 0,
            eof: false,
            state: State::Normal,
            stmt: Vec::new(),
            code: false,
            rows: Vec::new(),
            skip_line: false,
            out: VecDeque::new(),
        }
    }

    /// Adds bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        // What was read is dropped now and then, so a long dump does not pile up.
        if self.at > (1 << 20) && self.at * 2 > self.input.len() {
            self.input.drain(..self.at);
            self.at = 0;
        }
        self.input.extend_from_slice(bytes);
    }

    /// Says that there are no more bytes.
    ///
    /// # Errors
    /// [`SplitError::Unterminated`] when the dump stops inside a string, a comment or a
    /// `COPY`; other errors the last bytes may bring.
    pub fn finish(&mut self) -> Result<(), SplitError> {
        self.eof = true;
        self.scan()?;
        match self.state {
            State::Normal if !self.code => Ok(()),
            State::Normal => {
                // A last statement without its `;`.
                self.end_statement()
            }
            _ => Err(SplitError::Unterminated),
        }
    }

    /// The next item, or `None` when more bytes are needed.
    ///
    /// # Errors
    /// A [`SplitError`]; the reader is of no more use after one.
    pub fn next_item(&mut self) -> Result<Option<Item>, SplitError> {
        if self.out.is_empty() {
            self.scan()?;
        }
        Ok(self.out.pop_front())
    }

    fn peek(&self, offset: usize) -> Option<u8> {
        self.input.get(self.at + offset).copied()
    }

    /// Whether a byte `offset` ahead is needed and not here yet (more may come).
    fn short(&self, offset: usize) -> bool {
        !self.eof && self.at + offset >= self.input.len()
    }

    fn take(&mut self, n: usize) {
        for _ in 0..n {
            if let Some(b) = self.input.get(self.at).copied() {
                self.stmt.push(b);
                self.at += 1;
            }
        }
    }

    fn check_size(&self) -> Result<(), SplitError> {
        if self.stmt.len() > self.limit {
            Err(SplitError::TooLarge(self.limit))
        } else {
            Ok(())
        }
    }

    /// Reads as far as the bytes allow; stops at the first item.
    fn scan(&mut self) -> Result<(), SplitError> {
        while self.out.is_empty() {
            if self.at >= self.input.len() {
                break;
            }
            if self.state == State::Copy {
                if !self.copy_step()? {
                    break;
                }
                continue;
            }
            if !self.step()? {
                break;
            }
        }
        Ok(())
    }

    /// One line of `COPY` rows. `false` when a whole line is not here yet.
    fn copy_step(&mut self) -> Result<bool, SplitError> {
        if self.skip_line {
            let rest = &self.input[self.at..];
            match rest.iter().position(|b| *b == b'\n') {
                Some(nl) => {
                    self.at += nl + 1;
                    self.skip_line = false;
                    return Ok(true);
                }
                None if self.eof => return Err(SplitError::Unterminated),
                None => {
                    self.at = self.input.len();
                    return Ok(false);
                }
            }
        }
        let rest = &self.input[self.at..];
        let Some(nl) = rest.iter().position(|b| *b == b'\n') else {
            if rest.len() > self.limit {
                return Err(SplitError::TooLarge(self.limit));
            }
            if self.eof {
                return Err(SplitError::Unterminated);
            }
            return Ok(false);
        };
        let line = &rest[..nl];
        let bare = line.strip_suffix(b"\r").unwrap_or(line);
        if bare == b"\\." {
            if !self.rows.is_empty() {
                let rows = std::mem::take(&mut self.rows);
                self.out.push_back(Item::CopyData(rows));
            }
            self.out.push_back(Item::CopyEnd);
            self.at += nl + 1;
            self.state = State::Normal;
            return Ok(true);
        }
        self.rows.extend_from_slice(&rest[..=nl]);
        self.at += nl + 1;
        if self.rows.len() >= ROWS_PIECE {
            let rows = std::mem::take(&mut self.rows);
            self.out.push_back(Item::CopyData(rows));
        }
        Ok(true)
    }

    /// One step in the text. `false` when more bytes are needed to decide.
    fn step(&mut self) -> Result<bool, SplitError> {
        let Some(b) = self.peek(0) else {
            return Ok(false);
        };
        match self.state.clone() {
            State::Normal => self.normal(b),
            State::Single(escapes) => {
                if escapes && b == b'\\' {
                    if self.short(1) {
                        return Ok(false);
                    }
                    self.take(2);
                } else if b == b'\'' {
                    if self.short(1) {
                        return Ok(false);
                    }
                    if self.peek(1) == Some(b'\'') {
                        self.take(2);
                    } else {
                        self.take(1);
                        self.state = State::Normal;
                    }
                } else {
                    self.take(1);
                }
                self.check_size()?;
                Ok(true)
            }
            State::Double => {
                if b == b'"' {
                    if self.short(1) {
                        return Ok(false);
                    }
                    if self.peek(1) == Some(b'"') {
                        self.take(2);
                    } else {
                        self.take(1);
                        self.state = State::Normal;
                    }
                } else {
                    self.take(1);
                }
                self.check_size()?;
                Ok(true)
            }
            State::Line => {
                self.take(1);
                if b == b'\n' {
                    self.state = State::Normal;
                }
                self.check_size()?;
                Ok(true)
            }
            State::Block(depth) => {
                if b == b'/' && self.peek(1) == Some(b'*') {
                    self.take(2);
                    self.state = State::Block(depth + 1);
                } else if b == b'*' && self.peek(1) == Some(b'/') {
                    self.take(2);
                    self.state = if depth <= 1 {
                        State::Normal
                    } else {
                        State::Block(depth - 1)
                    };
                } else if (b == b'/' || b == b'*') && self.short(1) {
                    return Ok(false);
                } else {
                    self.take(1);
                }
                self.check_size()?;
                Ok(true)
            }
            State::Dollar(tag) => {
                if b == b'$' {
                    let close = tag.len() + 2;
                    if self.short(close - 1) {
                        return Ok(false);
                    }
                    let matches = self.peek(close - 1) == Some(b'$')
                        && self.input[self.at + 1..self.at + close - 1] == tag[..];
                    if matches {
                        self.take(close);
                        self.state = State::Normal;
                    } else {
                        self.take(1);
                    }
                } else {
                    self.take(1);
                }
                self.check_size()?;
                Ok(true)
            }
            State::Copy => Ok(false),
        }
    }

    fn normal(&mut self, b: u8) -> Result<bool, SplitError> {
        match b {
            b';' => {
                self.at += 1;
                self.end_statement()?;
            }
            b'\'' => {
                // `E'…'` (and `e'…'`) read backslash as an escape; a plain string does not.
                let n = self.stmt.len();
                let escapes = n >= 1
                    && matches!(self.stmt[n - 1], b'e' | b'E')
                    && (n == 1 || !is_word(self.stmt[n - 2]));
                self.take(1);
                self.code = true;
                self.state = State::Single(escapes);
            }
            b'"' => {
                self.take(1);
                self.code = true;
                self.state = State::Double;
            }
            b'-' => {
                if self.short(1) {
                    return Ok(false);
                }
                if self.peek(1) == Some(b'-') {
                    self.take(2);
                    self.state = State::Line;
                } else {
                    self.take(1);
                    self.code = true;
                }
            }
            b'/' => {
                if self.short(1) {
                    return Ok(false);
                }
                if self.peek(1) == Some(b'*') {
                    self.take(2);
                    self.state = State::Block(1);
                } else {
                    self.take(1);
                    self.code = true;
                }
            }
            b'$' => {
                // A dollar quote is `$tag$` (a tag is a word that does not start with a
                // digit, or nothing); `$1` is a parameter, not a quote.
                let mut len = 1;
                loop {
                    if self.short(len) {
                        return Ok(false);
                    }
                    match self.peek(len) {
                        Some(c) if is_word(c) && (len > 1 || !c.is_ascii_digit()) => len += 1,
                        _ => break,
                    }
                }
                if self.peek(len) == Some(b'$') && !self.prev_is_word() {
                    let tag = self.input[self.at + 1..self.at + len].to_vec();
                    self.take(len + 1);
                    self.code = true;
                    self.state = State::Dollar(tag);
                } else {
                    self.take(1);
                    self.code = true;
                }
            }
            b'\\' if !self.code && self.at_line_start() => {
                // A psql command on a line of its own.
                let rest = &self.input[self.at..];
                let Some(nl) = rest.iter().position(|c| *c == b'\n') else {
                    if self.eof {
                        let line = rest.to_vec();
                        self.at = self.input.len();
                        return self.meta(&line).map(|()| true);
                    }
                    return Ok(false);
                };
                let line = rest[..nl].to_vec();
                self.at += nl + 1;
                self.meta(&line)?;
            }
            c if c.is_ascii_whitespace() => self.take(1),
            _ => {
                self.take(1);
                self.code = true;
            }
        }
        self.check_size()?;
        Ok(true)
    }

    fn prev_is_word(&self) -> bool {
        self.stmt.last().is_some_and(|c| is_word(*c))
    }

    /// Whether what is behind us is the start of a line (or of the dump).
    fn at_line_start(&self) -> bool {
        let mut i = self.at;
        while i > 0 {
            match self.input[i - 1] {
                b'\n' => return true,
                b' ' | b'\t' | b'\r' => i -= 1,
                _ => return false,
            }
        }
        true
    }

    fn meta(&mut self, line: &[u8]) -> Result<(), SplitError> {
        let text = String::from_utf8_lossy(line);
        let word: String = text
            .trim_start_matches('\\')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .take(20)
            .collect();
        if matches!(word.as_str(), "restrict" | "unrestrict") {
            // Followed by a key of letters and digits, and nothing else.
            return Ok(());
        }
        Err(SplitError::MetaCommand(word))
    }

    fn end_statement(&mut self) -> Result<(), SplitError> {
        let raw = std::mem::take(&mut self.stmt);
        let had_code = std::mem::replace(&mut self.code, false);
        if !had_code {
            return Ok(());
        }
        let text = String::from_utf8(raw).map_err(|_| SplitError::NotText)?;
        if is_copy_from_stdin(&text) {
            self.out.push_back(Item::CopyStart(text));
            self.skip_line = true;
            self.state = State::Copy;
        } else {
            self.out.push_back(Item::Statement(text));
        }
        Ok(())
    }
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Whether a statement is `COPY … FROM stdin` (comments and spacing aside).
fn is_copy_from_stdin(statement: &str) -> bool {
    let mut words = Vec::new();
    let mut rest = statement;
    // Words outside comments, lower case; strings and quoted names are one odd word each
    // (they do not matter here: only the first word and the last two decide).
    while !rest.is_empty() {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("--") {
            rest = r.split_once('\n').map_or("", |(_, tail)| tail);
        } else if rest.starts_with("/*") {
            let mut depth = 0u32;
            let bytes = rest.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            rest = &rest[i.min(rest.len())..];
        } else {
            let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            words.push(rest[..end].to_ascii_lowercase());
            rest = &rest[end..];
        }
    }
    words.first().is_some_and(|w| w == "copy")
        && words.len() >= 3
        && words[words.len() - 2] == "from"
        && words[words.len() - 1] == "stdin"
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn all(text: &[u8], piece: usize) -> Result<Vec<Item>, SplitError> {
        let mut s = PgSplitter::new(1 << 20);
        let mut items = Vec::new();
        for chunk in text.chunks(piece) {
            s.push(chunk);
            while let Some(i) = s.next_item()? {
                items.push(i);
            }
        }
        s.finish()?;
        while let Some(i) = s.next_item()? {
            items.push(i);
        }
        // Rows are handed out in pieces: one run is one piece for these small dumps.
        Ok(items)
    }

    fn statements(items: &[Item]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Statement(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn statements_end_at_a_semicolon_that_is_not_quoted_or_commented() {
        let dump = b"-- a comment; with a semicolon\nSELECT 1;\n/* block; /* nested; */ still; */ SELECT ';' ;\nSELECT \"a;b\";\nSELECT E'it\\'s; fine';\nSELECT 'it''s; fine';\nCREATE FUNCTION f() RETURNS int AS $body$ BEGIN RETURN 1; END; $body$ LANGUAGE plpgsql;\nSELECT $1;\nSELECT 2";
        for piece in [1, 2, 3, 7, 1000] {
            let items = all(dump, piece).unwrap();
            let st = statements(&items);
            assert_eq!(st.len(), 8, "piece {piece}: {st:?}");
            assert!(st[0].contains("SELECT 1"), "{st:?}");
            assert!(st[1].ends_with("SELECT ';' "), "{st:?}");
            assert_eq!(st[2].trim(), "SELECT \"a;b\"");
            assert_eq!(st[3].trim(), "SELECT E'it\\'s; fine'");
            assert_eq!(st[4].trim(), "SELECT 'it''s; fine'");
            assert!(st[5].contains("BEGIN RETURN 1; END;"), "{st:?}");
            assert_eq!(st[6].trim(), "SELECT $1");
            assert_eq!(
                st[7].trim(),
                "SELECT 2",
                "a last statement needs no semicolon"
            );
        }
    }

    #[test]
    fn copy_rows_are_data_up_to_the_line_with_a_backslash_and_a_dot() {
        let dump = b"CREATE TABLE t (a int, b text);\nCOPY public.t (a, b) FROM stdin;\n1\tone\n2\t\\N\n3\ta;b -- not a comment\n\\.\nSELECT 1;\n";
        for piece in [1, 5, 1000] {
            let items = all(dump, piece).unwrap();
            let rows: Vec<u8> = items
                .iter()
                .filter_map(|i| match i {
                    Item::CopyData(d) => Some(d.clone()),
                    _ => None,
                })
                .flatten()
                .collect();
            assert_eq!(
                rows, b"1\tone\n2\t\\N\n3\ta;b -- not a comment\n",
                "piece {piece}"
            );
            assert!(matches!(items[1], Item::CopyStart(_)));
            assert!(items.contains(&Item::CopyEnd));
            assert_eq!(statements(&items).len(), 2);
            // After the rows the text goes on as SQL.
            assert!(matches!(items.last(), Some(Item::Statement(s)) if s.trim() == "SELECT 1"));
        }
    }

    #[test]
    fn a_copy_that_looks_like_one_inside_a_string_or_comment_is_not_one() {
        let dump = b"SELECT 'COPY t FROM stdin;';\n-- COPY t FROM stdin;\nSELECT 2;\n";
        let items = all(dump, 3).unwrap();
        assert!(!items.iter().any(|i| matches!(i, Item::CopyStart(_))));
        assert_eq!(statements(&items).len(), 2);
    }

    #[test]
    fn psql_commands_are_refused_except_the_restrict_lines_of_pg_dump() {
        let ok = b"\\restrict AbC123\nSELECT 1;\n\\unrestrict AbC123\n";
        let items = all(ok, 4).unwrap();
        assert_eq!(statements(&items), ["SELECT 1"]);
        for bad in [
            &b"\\connect other\n"[..],
            b"SELECT 1;\n\\! id\nSELECT 2;\n",
            b"\\i /etc/passwd\n",
            b"  \\o /tmp/x\n",
        ] {
            let err = all(bad, 3).unwrap_err();
            assert!(matches!(err, SplitError::MetaCommand(_)), "{err}");
        }
        // A backslash in the middle of a statement is only a byte.
        assert_eq!(
            statements(&all(b"SELECT 1 \\ 2;", 2).unwrap()),
            ["SELECT 1 \\ 2"]
        );
    }

    #[test]
    fn what_is_cut_off_or_too_big_is_an_error() {
        assert_eq!(
            all(b"SELECT 'open", 3).unwrap_err(),
            SplitError::Unterminated
        );
        assert_eq!(all(b"/* open", 3).unwrap_err(), SplitError::Unterminated);
        assert_eq!(
            all(b"COPY t FROM stdin;\n1\n", 3).unwrap_err(),
            SplitError::Unterminated
        );
        assert_eq!(
            all(&[b'S', b'E', 0xff, 0xfe, b';'], 2).unwrap_err(),
            SplitError::NotText
        );
        let mut s = PgSplitter::new(10);
        s.push(b"SELECT 'a long string that goes on';");
        assert_eq!(s.next_item().unwrap_err(), SplitError::TooLarge(10));
    }

    #[test]
    fn copy_detection_ignores_spacing_and_comments_only() {
        assert!(is_copy_from_stdin("COPY public.t (a, b) FROM stdin"));
        assert!(is_copy_from_stdin("  copy t\n FROM\n STDIN"));
        assert!(is_copy_from_stdin("/* x */ COPY t FROM stdin -- y"));
        assert!(!is_copy_from_stdin("COPY t FROM '/etc/passwd'"));
        assert!(!is_copy_from_stdin("COPY t TO stdout"));
        assert!(!is_copy_from_stdin("SELECT 'copy t from stdin'"));
    }
}
