//! Cuts an SQL dump into statements (T07.07).
//!
//! Reads bytes as they come and hands out one statement at a time, the way the
//! `mysql` client does: statements end at the delimiter (`;` unless a
//! `DELIMITER` line changed it, as `mysqldump` does around triggers and
//! routines); a delimiter inside a string (`'…'`, `"…"` with backslash escapes),
//! a quoted name (`` `…` ``) or a comment (`-- …`, `# …`, `/* … */`, which includes
//! the executable `/*! … */`) does not count. Comments stay with their statement
//! (the server ignores them); a stretch that is only comments and white space is
//! not a statement. A statement larger than the limit is an error, never run.

/// Why reading stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SplitError {
    /// One statement is longer than the limit.
    #[error("a statement is larger than {0} bytes")]
    TooLarge(usize),
    /// The dump ends in the middle of a string or comment.
    #[error("the dump ends inside a string, a quoted name or a comment")]
    Unterminated,
    /// Not text the server could read.
    #[error("the dump is not valid UTF-8 text")]
    NotText,
    /// A `DELIMITER` line that cannot be used.
    #[error("a DELIMITER line is empty, too long or has a backslash")]
    BadDelimiter,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    Single,
    Double,
    Backtick,
    LineComment,
    BlockComment,
}

/// The reader. Feed it bytes with [`Splitter::push`], take statements with
/// [`Splitter::next_statement`], and call [`Splitter::finish`] at the end.
pub struct Splitter {
    limit: usize,
    delimiter: Vec<u8>,
    buf: Vec<u8>,
    state: State,
    /// Inside a string: the last byte was a backslash.
    escaped: bool,
    pending: std::collections::VecDeque<Vec<u8>>,
    /// Bytes handed to [`Splitter::push`] that are not yet scanned.
    scan_from: usize,
    error: Option<SplitError>,
}

impl Splitter {
    /// A reader that refuses statements over `limit` bytes.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            delimiter: b";".to_vec(),
            buf: Vec::new(),
            state: State::Normal,
            escaped: false,
            pending: std::collections::VecDeque::new(),
            scan_from: 0,
            error: None,
        }
    }

    /// Adds bytes of the dump.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
        self.scan();
    }

    /// The next whole statement, if there is one.
    ///
    /// # Errors
    /// [`SplitError`] when the dump cannot be read on.
    pub fn next_statement(&mut self) -> Result<Option<String>, SplitError> {
        if let Some(statement) = self.pending.pop_front() {
            return String::from_utf8(statement)
                .map(Some)
                .map_err(|_| SplitError::NotText);
        }
        match &self.error {
            Some(e) => Err(e.clone()),
            None => Ok(None),
        }
    }

    /// Ends the dump: whatever is left is the last statement (a final statement
    /// without its `;`), unless it is only comments.
    ///
    /// # Errors
    /// [`SplitError::Unterminated`] when the dump ends inside a string or comment.
    pub fn finish(&mut self) -> Result<(), SplitError> {
        self.scan();
        if self.error.is_some() {
            return Ok(());
        }
        match self.state {
            State::Normal | State::LineComment => {}
            _ => return Err(SplitError::Unterminated),
        }
        let rest = std::mem::take(&mut self.buf);
        self.scan_from = 0;
        if !is_blank(&rest) {
            self.pending.push_back(trim(rest));
        }
        Ok(())
    }

    fn scan(&mut self) {
        let mut i = self.scan_from;
        while i < self.buf.len() && self.error.is_none() {
            let c = self.buf[i];
            match self.state {
                State::Normal => {
                    if self.at_statement_start(i)
                        && let Some(consumed) = self.delimiter_line(i)
                    {
                        match consumed {
                            Ok(end) => {
                                self.buf.drain(..end);
                                i = 0;
                                continue;
                            }
                            Err(()) if self.buf[i..].contains(&b'\n') => {
                                self.error = Some(SplitError::BadDelimiter);
                                return;
                            }
                            // The line is not complete yet.
                            Err(()) => break,
                        }
                    }
                    match c {
                        b'\'' => self.state = State::Single,
                        b'"' => self.state = State::Double,
                        b'`' => self.state = State::Backtick,
                        b'#' => self.state = State::LineComment,
                        // `--` starts a comment only when white space follows: wait for
                        // the bytes that decide.
                        b'-' if i + 1 >= self.buf.len()
                            || (self.buf[i + 1] == b'-' && i + 2 >= self.buf.len()) =>
                        {
                            break;
                        }
                        b'-' if self.buf[i + 1] == b'-'
                            && self.buf[i + 2].is_ascii_whitespace() =>
                        {
                            self.state = State::LineComment;
                        }
                        b'/' if self.buf.get(i + 1) == Some(&b'*') => {
                            self.state = State::BlockComment;
                            i += 1;
                        }
                        b'/' if self.buf.get(i + 1).is_none() => break,
                        _ => {}
                    }
                    // The delimiter ends the statement.
                    if self.state == State::Normal
                        && self.buf[..=i].ends_with(&self.delimiter)
                        && i + 1 >= self.delimiter.len()
                    {
                        let end = i + 1 - self.delimiter.len();
                        let statement: Vec<u8> = self.buf[..end].to_vec();
                        self.buf.drain(..=i);
                        i = 0;
                        if !is_blank(&statement) {
                            self.pending.push_back(trim(statement));
                        }
                        continue;
                    }
                }
                State::Single | State::Double => {
                    let quote = if self.state == State::Single {
                        b'\''
                    } else {
                        b'"'
                    };
                    if self.escaped {
                        self.escaped = false;
                    } else if c == b'\\' {
                        self.escaped = true;
                    } else if c == quote {
                        // A doubled quote stays inside the string: it reopens at once.
                        self.state = State::Normal;
                    }
                }
                State::Backtick => {
                    if c == b'`' {
                        self.state = State::Normal;
                    }
                }
                State::LineComment => {
                    if c == b'\n' {
                        self.state = State::Normal;
                    }
                }
                State::BlockComment => {
                    if c == b'*' && self.buf.get(i + 1) == Some(&b'/') {
                        self.state = State::Normal;
                        i += 1;
                    } else if c == b'*' && self.buf.get(i + 1).is_none() {
                        break;
                    }
                }
            }
            if self.buf.len() > self.limit && self.pending.is_empty() {
                self.error = Some(SplitError::TooLarge(self.limit));
                return;
            }
            i += 1;
        }
        self.scan_from = i.min(self.buf.len());
        if self.buf.len() > self.limit && self.pending.is_empty() && self.error.is_none() {
            self.error = Some(SplitError::TooLarge(self.limit));
        }
    }

    /// Whether only white space (and nothing of a statement) precedes `i`.
    fn at_statement_start(&self, i: usize) -> bool {
        self.buf[..i].iter().all(u8::is_ascii_whitespace)
    }

    /// A `DELIMITER xx` line starting at `i`: `Ok(end of the line)` after setting
    /// the delimiter, `Err` when it is not a complete valid one yet.
    fn delimiter_line(&mut self, i: usize) -> Option<Result<usize, ()>> {
        const WORD: &[u8] = b"delimiter";
        let rest = &self.buf[i..];
        if rest.len() <= WORD.len() {
            // Not enough yet to tell: wait while what is there could still become it.
            return WORD[..rest.len()]
                .eq_ignore_ascii_case(rest)
                .then_some(Err(()));
        }
        if !rest[..WORD.len()].eq_ignore_ascii_case(WORD) {
            return None;
        }
        // Must be followed by a space: `delimiters` is a name.
        match rest.get(WORD.len()) {
            Some(b' ' | b'\t') => {}
            _ => return None,
        }
        let Some(nl) = rest.iter().position(|b| *b == b'\n') else {
            return Some(Err(()));
        };
        let value = rest[WORD.len()..nl]
            .iter()
            .copied()
            .map(|b| if b == b'\r' { b' ' } else { b })
            .collect::<Vec<u8>>();
        let value = value.trim_ascii();
        if value.is_empty() || value.len() > 16 || value.contains(&b'\\') || value.contains(&b' ') {
            return Some(Err(()));
        }
        self.delimiter = value.to_vec();
        Some(Ok(i + nl + 1))
    }
}

fn trim(mut bytes: Vec<u8>) -> Vec<u8> {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    bytes.drain(..start);
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes.pop();
    }
    bytes
}

/// Whether the text is only white space and plain comments (nothing to run).
fn is_blank(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'#' || (c == b'-' && bytes.get(i + 1) == Some(&b'-')) {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'*') && bytes.get(i + 2) != Some(&b'!') {
            match bytes[i + 2..].windows(2).position(|w| w == b"*/") {
                Some(end) => i += 2 + end + 2,
                None => return false,
            }
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn all(text: &str) -> Vec<String> {
        let mut s = Splitter::new(1 << 20);
        s.push(text.as_bytes());
        s.finish().unwrap();
        let mut out = Vec::new();
        while let Some(st) = s.next_statement().unwrap() {
            out.push(st);
        }
        out
    }

    /// The same, feeding one byte at a time (the dump arrives in pieces).
    fn bytewise(text: &str) -> Vec<String> {
        let mut s = Splitter::new(1 << 20);
        let mut out = Vec::new();
        for b in text.as_bytes() {
            s.push(&[*b]);
            while let Some(st) = s.next_statement().unwrap() {
                out.push(st);
            }
        }
        s.finish().unwrap();
        while let Some(st) = s.next_statement().unwrap() {
            out.push(st);
        }
        out
    }

    const DUMP: &str = "-- MariaDB dump\n\
        /*!40101 SET @OLD_CHARACTER_SET_CLIENT=@@CHARACTER_SET_CLIENT */;\n\
        DROP TABLE IF EXISTS `t;1`;\n\
        CREATE TABLE `t;1` (a int, b text);\n\
        INSERT INTO `t;1` VALUES (1,'a;b'),(2,'it''s; ok'),(3,\"x\\\";y\"),(4,'back\\\\');\n\
        # a hash comment; with a semicolon\n\
        INSERT INTO `t;1` VALUES (5,'/* not a comment ; */'); -- trailing; comment\n\
        /* block ; comment */ SELECT 1;\n";

    #[test]
    fn statements_end_only_at_a_delimiter_outside_strings_names_and_comments() {
        let got = all(DUMP);
        assert_eq!(got.len(), 6, "{got:#?}");
        assert!(got[0].starts_with("-- MariaDB dump"));
        assert!(got[0].ends_with("*/"));
        assert_eq!(got[1], "DROP TABLE IF EXISTS `t;1`");
        assert!(
            got[3].contains("'it''s; ok'")
                && got[3].contains("\"x\\\";y\"")
                && got[3].contains("'back\\\\'")
        );
        assert!(got[4].contains("'/* not a comment ; */'"));
        // The comment after the previous statement on its line goes with the next one.
        assert_eq!(
            got[5],
            "-- trailing; comment\n/* block ; comment */ SELECT 1"
        );
        assert_eq!(bytewise(DUMP), got, "pieces make no difference");
    }

    #[test]
    fn delimiter_lines_change_the_end_of_a_statement_as_mysqldump_writes_them() {
        let text = "SELECT 1;\n\
            DELIMITER ;;\n\
            CREATE TRIGGER t BEFORE INSERT ON x FOR EACH ROW BEGIN\n  SET NEW.a = 1;\n  SET NEW.b = 2;\nEND ;;\n\
            CREATE PROCEDURE p() BEGIN SELECT ';;' ; END ;;\n\
            DELIMITER ;\n\
            SELECT 2;\n";
        let got = all(text);
        assert_eq!(got.len(), 4, "{got:#?}");
        assert_eq!(got[0], "SELECT 1");
        assert!(
            got[1].starts_with("CREATE TRIGGER")
                && got[1].contains("SET NEW.a = 1;")
                && got[1].ends_with("END")
        );
        assert!(got[2].contains("SELECT ';;' ; END"));
        assert_eq!(got[3], "SELECT 2");
        assert_eq!(bytewise(text), got);
        // CRLF line ends.
        assert_eq!(all(&text.replace('\n', "\r\n")).len(), 4);
        // `delimiters` is a name, not a command.
        assert_eq!(
            all("SELECT delimiters FROM t;\n"),
            ["SELECT delimiters FROM t"]
        );
    }

    #[test]
    fn a_last_statement_without_its_terminator_counts_and_comments_alone_do_not() {
        assert_eq!(all("SELECT 1; SELECT 2"), ["SELECT 1", "SELECT 2"]);
        assert!(all("-- only a comment\n/* and another */\n  \n").is_empty());
        assert!(all("SELECT 1;\n-- the end\n").len() == 1);
        // An executable comment is a statement.
        assert_eq!(all("/*!40101 SET x=1 */;").len(), 1);
    }

    #[test]
    fn what_cannot_be_run_is_an_error_and_never_a_partial_statement() {
        let mut s = Splitter::new(1 << 20);
        s.push(b"SELECT 'open");
        assert_eq!(s.finish(), Err(SplitError::Unterminated));
        let mut s = Splitter::new(1 << 20);
        s.push(b"SELECT 1; /* open");
        assert_eq!(s.finish(), Err(SplitError::Unterminated));
        // Too large: refused as soon as it is seen, before the end.
        let mut s = Splitter::new(100);
        s.push(b"INSERT INTO t VALUES ('");
        s.push(&[b'x'; 200]);
        assert_eq!(s.next_statement(), Err(SplitError::TooLarge(100)));
        // A statement under the limit after big ones is fine.
        let mut s = Splitter::new(100);
        s.push(b"SELECT 1;");
        assert_eq!(s.next_statement().unwrap().as_deref(), Some("SELECT 1"));
        // Not text.
        let mut s = Splitter::new(100);
        s.push(b"SELECT '\xff';");
        assert_eq!(s.next_statement(), Err(SplitError::NotText));
        // Bad delimiters.
        for bad in [
            "DELIMITER \\\n",
            "DELIMITER \n",
            "DELIMITER aaaaaaaaaaaaaaaaaaaaaaaa\n",
        ] {
            let mut s = Splitter::new(100);
            s.push(bad.as_bytes());
            assert_eq!(s.next_statement(), Err(SplitError::BadDelimiter), "{bad:?}");
        }
    }
}
