// == Std crates
use std::{io, io::BufRead};

// == Internal crates
use super::*;

// == External crates

#[derive(Debug)]
pub struct P4ZtagParser<ReadT: io::Read> {
    buffered_reader: io::BufReader<ReadT>,
    current_dict_index: Option<u32>,
    state: ZtagParseState,
    line_buffer: String,
    pending_line_buffer: Option<String>,
    dict_delimiter_key: Option<&'static str>,
}

#[derive(Debug, PartialEq)]
enum ZtagParseState {
    Root,              // Root state, next can be a dict or eof
    SingleLineYield,   // Single line yield state, we can yield the current line
    MultiLineYield, // Multiline yield state, we can yield the current line, but we need to keep the next line
    MultiLineInternal, // Multiline internal state, we are in a multiline var, and we may need to keep reading
    EmptyLine,         // Empty line, ignore
    Eof,               // End of file state, terminal
}

impl ZtagParseState {
    fn is_record_complete(&self) -> bool {
        matches!(
            self,
            ZtagParseState::Root
                | ZtagParseState::SingleLineYield
                | ZtagParseState::MultiLineYield
                | ZtagParseState::EmptyLine
        )
    }

    fn should_yield(&self) -> bool {
        matches!(
            self,
            ZtagParseState::SingleLineYield | ZtagParseState::MultiLineYield
        )
    }
}

impl<ReadT: io::Read + std::fmt::Debug> P4KvpStream<io::Error> for P4ZtagParser<ReadT> {
    fn get_next_kvp<'b>(&'b mut self) -> Result<Option<P4KeyValuePair<'b>>, io::Error> {
        self.get_next_kvp()
    }
}

impl<ReadT: io::Read + std::fmt::Debug> P4ZtagParser<ReadT> {
    // These are the variables that can be multiline, and we need to handle them specially
    const MULTILINE_VAR_PREFIXES: [&str; 1] = ["... desc "];
    const PREFIX: &str = "... ";
    const PREFIX_LEN: usize = Self::PREFIX.len();

    pub fn new(reader: ReadT, dict_delimiter_key: Option<&'static str>) -> Self {
        P4ZtagParser {
            buffered_reader: io::BufReader::new(reader),
            current_dict_index: None,
            state: ZtagParseState::Root,
            line_buffer: String::default(),
            pending_line_buffer: None,
            dict_delimiter_key,
        }
    }

    pub fn get_next_kvp<'b>(&'b mut self) -> Result<Option<P4KeyValuePair<'b>>, io::Error> {
        loop {
            let state = self.advance()?;
            //println!("State: {:?} -> {:?}", state, self);
            self.state = state;

            if self.state.should_yield() {
                // We have a kvp, yield it
                let (key, value) = Self::get_kvp_refs(&self.line_buffer)?;

                // For ztag, we increment the dict index BEFORE we yield, since we update on the first delimited key
                if Some(key) == self.dict_delimiter_key {
                    if self.current_dict_index.is_none() {
                        self.current_dict_index = Some(0);
                    } else {
                        *self.current_dict_index.as_mut().unwrap() += 1;
                    };
                }

                let result = Ok(Some(P4KeyValuePair {
                    dict_index: self.current_dict_index.unwrap_or(0),
                    key,
                    value,
                }));

                return result;
            }

            if self.state == ZtagParseState::Eof {
                break;
            }
        }

        Ok(None)
    }

    fn get_kvp_refs<'a>(line_buffer: &'a String) -> Result<(&'a str, &'a str), io::Error> {
        // If we're here, we have a new line to process, it _should_ always start with '... '
        assert!(
            line_buffer.starts_with(Self::PREFIX),
            "Line does not start with prefix: {}",
            line_buffer
        );

        // New field
        if let Some(key_end) = line_buffer[Self::PREFIX_LEN..].find(" ") {
            let key = &line_buffer[Self::PREFIX_LEN..key_end + Self::PREFIX_LEN];
            // We need to trim the trailing \n and possibly the trailing \r
            let trim_index = if &line_buffer[line_buffer.len() - 2..line_buffer.len()] == "\r\n" {
                line_buffer.len() - 2
            } else {
                line_buffer.len() - 1
            };

            let value = &line_buffer[Self::PREFIX_LEN + key_end + 1..trim_index];
            Ok((key, value))
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Line does not contain a key-value pair",
            ))
        }
    }

    // Returns true if we should yield the line, false if we should continue reading
    fn advance(&mut self) -> Result<ZtagParseState, io::Error> {
        // If we're in a multiline var, there are two possibilities
        // 1. If the next line starts with a the ... prefix, then we're done and need to yield
        // 2. If the line doesn't start with ... we need to just append

        assert_ne!(
            self.state,
            ZtagParseState::Eof,
            "State should not be EOF here"
        );

        // If we have a pending line, that means it is a new record and the last one is complete
        if let Some(pending_line) = self.pending_line_buffer.take() {
            self.line_buffer = pending_line;
        } else if self.state.is_record_complete() {
            // This means we're at a new record
            self.line_buffer.clear();
            if self.buffered_reader.read_line(&mut self.line_buffer)? == 0 {
                // End of file
                return Ok(ZtagParseState::Eof);
            } else if self.line_buffer.len() == 1
                && self.line_buffer.chars().nth(0).unwrap() == '\n'
            {
                return Ok(ZtagParseState::EmptyLine);
            } else {
                // No-op here, new record common processing finishes below
            }
        } else {
            // This means we might be at a new record OR a continuation of the previous one
            let mut next_line = String::default();
            if self.buffered_reader.read_line(&mut next_line)? == 0 {
                // End of file, but we need to yield the current record first, next round will return EOF
                return Ok(ZtagParseState::MultiLineYield);
            } else if next_line.starts_with(Self::PREFIX) {
                // We have a new line, so we can yield the previous one, BUT we need to keep the next line so we can yield that next
                self.pending_line_buffer = Some(next_line);
                return Ok(ZtagParseState::MultiLineYield);
            } else {
                // Continuation of other line
                self.line_buffer.push_str(&next_line);
                // New record common processing finishes below
            }
        }

        if Self::MULTILINE_VAR_PREFIXES
            .iter()
            .any(|prefix| self.line_buffer.starts_with(prefix))
        {
            Ok(ZtagParseState::MultiLineInternal)
        } else {
            Ok(ZtagParseState::SingleLineYield)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ztag_parsing() {
        let data = "\
            ... changeType public\n\
            ... change 12345\n\
            ... desc BLAHBLAH\n\
            BLAHBLAH\n\
            ... zambo aaa\n\
            ... zoop bbb\n\
            \n\
            ... desc WOOWOO\n\
            WOWWOW\n\
            ... desc SNASNA\n\
            ... desc SNASNA2\n";

        let expected = [
            ("changeType", "public", 0),
            ("change", "12345", 0),
            ("desc", "BLAHBLAH\nBLAHBLAH", 0),
            ("zambo", "aaa", 1),
            ("zoop", "bbb", 1),
            ("desc", "WOOWOO\nWOWWOW", 1),
            ("desc", "SNASNA", 2),
            ("desc", "SNASNA2", 3),
        ]
        .map(|(key, value, dict_index)| P4KeyValuePair {
            dict_index,
            key,
            value,
        });

        let reader = data.as_bytes();
        let mut parser = P4ZtagParser::new(reader, Some("desc"));

        let mut index = 0;
        while let Some(kvp) = parser.get_next_kvp().unwrap() {
            //println!("{}: {} -> {}", kvp.dict_index, kvp.key, kvp.value);
            assert_eq!(
                kvp, expected[index],
                "Key-value pair mismatch at index {}",
                index
            );

            index += 1;
        }

        assert_eq!(index, expected.len(), "Not all key-value pairs were read");
    }
}
