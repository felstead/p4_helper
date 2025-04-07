
// == Std crates
use std::io;

// == Internal crates
use super::*;

// == External crates
use thiserror::Error;

// == Private, inner types
#[derive(Debug, PartialEq)]
enum PyDictParseState {
    Root,   // Root state, next can be a dict or eof
    Dict,   // Inner dict state, next can be a string or null
    Key,    // Key string state, next can be a string
    Value,  // Value string state, next can be a string or null
    Eof     // End of file state, terminal
}

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum PyDictTag {
    Dict,   // {
    String, // s
    Null,   // 0
    Other,  // Any other byte
    Eof,    // End of file
}

impl PyDictTag {
    fn from_byte(byte: u8) -> Self {
        match byte {
            b'{' => PyDictTag::Dict,
            b's' => PyDictTag::String,
            b'0' => PyDictTag::Null,
            _ => PyDictTag::Other,
        }
    }
}

// == Public types
#[derive(Debug, Error)]
#[error("P4PyDictParseError")]
pub enum P4PyDictParseError {
    UnexpectedEof,
    InvalidTag { tag: u8 },
    Io(io::Error),
}

pub struct P4PyDictParser<ReadT: io::Read> {
    reader: ReadT,
    state: PyDictParseState,
    current_dict_index: Option<u32>,
    // Owned buffers we can re-use so we can just return references to kvps as they stream in
    current_key_buffer: Vec<u8>,
    current_value_buffer: Vec<u8>,
}

impl<ReadT: io::Read> P4KvpStream<P4PyDictParseError> for P4PyDictParser<ReadT> {
    fn get_next_kvp<'b>(&'b mut self) -> Result<Option<P4KeyValuePair<'b>>, P4PyDictParseError> {
        self.get_next_kvp()
    }
}

impl<ReadT: io::Read> P4PyDictParser<ReadT> {
    pub fn new(reader: ReadT) -> Self {
        P4PyDictParser {
            reader,
            state: PyDictParseState::Root,
            current_dict_index: None,
            current_key_buffer: Vec::with_capacity(1024),
            current_value_buffer: Vec::with_capacity(1024),
        }
    }

    pub fn get_next_kvp<'b>(&'b mut self) -> Result<Option<P4KeyValuePair<'b>>, P4PyDictParseError> {
        // Loop until we find a key-value pair
        while self.state != PyDictParseState::Eof {
            if self.advance()? {
                // We have a kvp, yield it
                let kvp = P4KeyValuePair {
                    dict_index: self.current_dict_index.unwrap(),
                    key: std::str::from_utf8(&self.current_key_buffer).unwrap(),
                    value: std::str::from_utf8(&self.current_value_buffer).unwrap(),
                };

                return Ok(Some(kvp));
            }
        }

        Ok(None)
    }

    fn advance(&mut self) -> Result<bool, P4PyDictParseError> {
        let mut should_yield = false;
        self.state = match self.state {
            PyDictParseState::Root => {
                // We can have a dict or nothing in the root state
                match self.expect_tags(&[PyDictTag::Dict, PyDictTag::Eof])? {
                    PyDictTag::Dict => {
                        if self.current_dict_index.is_none() {
                            self.current_dict_index = Some(0);
                        } else {
                            *self.current_dict_index.as_mut().unwrap() += 1;
                        }

                        PyDictParseState::Dict
                    },
                    PyDictTag::Eof => PyDictParseState::Eof,
                    _ => unreachable!(),
                }
            },
            PyDictParseState::Dict => {
                // We can have a string or a null (closing dict) in the dict state
                match self.expect_tags(&[PyDictTag::String, PyDictTag::Null])? {
                    PyDictTag::String => PyDictParseState::Key,
                    PyDictTag::Null => {
                        // Dict is closed, so we can increment the dict index
                        PyDictParseState::Root
                    },
                    _ => unreachable!(),
                }
            },
            PyDictParseState::Key => {
                // Extract the string
                Self::read_string(&mut self.reader, &mut self.current_key_buffer)?;

                // Single variant, no need to check, the ? operator will bubble up a bad tag
                self.expect_tags(&[PyDictTag::String])?;
                PyDictParseState::Value
            }
            PyDictParseState::Value => {
                // Extract the string
                Self::read_string(&mut self.reader, &mut self.current_value_buffer)?;

                // Yield the KVP
                should_yield = true;

                match self.expect_tags(&[PyDictTag::String, PyDictTag::Null])? {
                    PyDictTag::String => PyDictParseState::Key,
                    PyDictTag::Null => PyDictParseState::Root,
                    _ => unreachable!(),
                }
            },
            PyDictParseState::Eof => {
                unreachable!()
            }
        };

        Ok(should_yield)
    }

    fn expect_tags(&mut self, tags : &'static [PyDictTag]) -> Result<PyDictTag, P4PyDictParseError> {
        let mut type_buffer = [0u8; 1];
        match self.reader.read_exact(&mut type_buffer) {
            Ok(_) => {
                let found_tag = PyDictTag::from_byte(type_buffer[0]);
                if tags.iter().any(|&tag| tag == found_tag) {
                    return Ok(found_tag);
                } else {
                    return Err(P4PyDictParseError::InvalidTag { tag: type_buffer[0] });
                }
            },
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => { Ok(PyDictTag::Eof) },
            Err(e) => Err(P4PyDictParseError::Io(e))
        }
    }

    // We receive the string with the reader already past the 's' tag at the beginning, so are expecting '<LEN:u32_le>[u8;LEN]'
    fn read_string(reader : &mut ReadT, buffer : &mut Vec<u8>) -> Result<(), P4PyDictParseError> {
        buffer.clear();
        
        let mut len_buffer = [0u8; 4];
        // Read the length of the string
        let len = match reader.read_exact(&mut len_buffer) {
            Ok(_) => Ok(u32::from_le_bytes(len_buffer)),
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => { Err(P4PyDictParseError::UnexpectedEof) },
            Err(e) => Err(P4PyDictParseError::Io(e))
        }?;

        // Read the string
        buffer.resize(len as usize, 0);

        match reader.read_exact(&mut buffer[..]) {
            Ok(_) => Ok(()),
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => { Err(P4PyDictParseError::UnexpectedEof) },
            Err(e) => Err(P4PyDictParseError::Io(e))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_reader() {
        let mut reader = fs::File::open("C:\\EpicGames\\longyeet.pyc").unwrap();
        let mut parser = P4PyDictParser::new(&mut reader);

        while let Some(kvp) = parser.get_next_kvp().unwrap() {
            println!("{:?}", kvp);
        }
    }
}
