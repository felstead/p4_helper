// == Std crates
use std::process;

// == Internal crates
use crate::*;
use crate::parsers::py_dict::P4PyDictParser;

pub struct P4DescribeIterator {
    p4_process: process::Child,
    parser: P4PyDictParser<process::ChildStdout>,
    changelist: P4Changelist,
    // Storage for various state variables
    current_file_index: Option<u32>,
    current_file: InterimP4File,
}

impl P4DescribeIterator {
    pub fn new(changelist: u32) -> Result<Self, &'static str> {
        let mut p4_process = get_p4_cmd(vec!["describe", "-s", &format!("{}", changelist)])
            .spawn()
            .expect("Failed to start p4 command");

        let reader = p4_process.stdout.take().expect("Failed to get stdout of p4 command");
        let mut parser = P4PyDictParser::new(reader);

        let mut current_file_index = None;
        let mut current_change = InterimP4Changelist::default();
        let mut current_file = InterimP4File::default();

        // Read the first parts to get the CL information
        while let Some(kvp) = parser.get_next_kvp().unwrap() {
            match kvp.key {
                "change" => { current_change.change = Some(kvp.value.parse().unwrap()); },
                "time" => { current_change.time = Some(kvp.value.parse().unwrap()); },
                "user" => { current_change.user = Some(kvp.value.to_string()); },
                "desc" => { current_change.description = Some(kvp.value.to_string()); },
                key => { 
                    if let Some((key, index)) = split_indexed_key(key) {
                        Self::populate_field(&mut current_file, key, kvp.value);
                        current_file_index = Some(index);
                        break;
                    }
                }
            }
        }

        let changelist = current_change.try_into()?;

        // We are done with the header, so we can store it
        Ok(P4DescribeIterator {
            p4_process,
            parser,
            changelist,
            current_file_index,
            current_file,
        })
    }

    pub fn get_changelist(&self) -> &P4Changelist {
        &self.changelist
    }

    fn populate_field(file: &mut InterimP4File, key: &str, value: &str) {
        match key {
            "depotFile" => { file.depot_path = Some(value.to_string()); },
            "action" => { file.action = Some(value.to_string()); },
            "rev" => { file.revision = Some(value.parse().unwrap()); },
            "fileSize" => { file.file_size = Some(value.parse().unwrap()); },
            "digest" => { file.digest = Some(const_hex::decode_to_array(value).unwrap()) },
            _ => { } // No op
        }
    }
}

impl Iterator for P4DescribeIterator {
    type Item = P4File;

    fn next(&mut self) -> Option<Self::Item> {
        // Read the next file from the p4 process
        while let Some(kvp) = self.parser.get_next_kvp().unwrap() {
            if let Some((key, index)) = split_indexed_key(kvp.key) {
                if Some(index) != self.current_file_index {
                    self.current_file_index = Some(index);

                    // We are done with the current record, so we can yield it
                    let file = std::mem::take(&mut self.current_file).try_into().unwrap();

                    // We still need to process this pair for the next file
                    Self::populate_field(&mut self.current_file, key, kvp.value);

                    return Some(file);
                }

                Self::populate_field(&mut self.current_file, key, kvp.value);
            } else {
                // This shouldn't happen, so warn?
                panic!("Unexpected key format: {}", kvp.key);
            }

        }

        // Yield the last file
        if self.current_file_index.is_some() {
            let file = std::mem::take(&mut self.current_file).try_into().unwrap();
            self.current_file_index = None;
            return Some(file);
        }

        self.p4_process.wait().unwrap(); // Wait for the p4 process to finish
        None
    }
}
