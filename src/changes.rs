// == Std crates
use std::{io, ops::Range};

// == Internal crates
use crate::parsers::py_dict::*;
use crate::*;

pub struct P4ChangesIterator<ReadT: io::Read> {
    p4_process: Option<process::Child>,
    parser: P4PyDictParser<ReadT>,
    // Storage for various state variables
    previous_dict_index: Option<u32>,
    current_change: InterimP4Changelist,
}

impl<ReadT: io::Read> P4ChangesIterator<ReadT> {
    pub fn new_from_p4_exe(
        cl_range: Option<Range<u32>>,
    ) -> Result<P4ChangesIterator<process::ChildStdout>, &'static str> {
        let cl_range = cl_range.unwrap_or(0..u32::MAX);

        let mut p4_process = get_p4_cmd(vec![
            "changes",
            "-s",
            "submitted",
            "-l",
            &format!("@{},{}", cl_range.start, cl_range.end),
        ])
        .spawn()
        .expect("Failed to start p4 command");

        let reader = p4_process
            .stdout
            .take()
            .expect("Failed to get stdout of p4 command");
        let parser = P4PyDictParser::new(reader);

        Ok(P4ChangesIterator {
            p4_process: Some(p4_process),
            parser,
            previous_dict_index: Some(0),
            current_change: InterimP4Changelist::default(),
        })
    }

    pub fn new_from_reader(reader: ReadT) -> P4ChangesIterator<ReadT> {
        let parser = P4PyDictParser::new(reader);

        P4ChangesIterator {
            p4_process: None,
            parser,
            previous_dict_index: Some(0),
            current_change: InterimP4Changelist::default(),
        }
    }

    fn populate_field(change: &mut InterimP4Changelist, key: &str, value: &str) {
        match key {
            "change" => {
                change.change = Some(value.parse().unwrap());
            }
            "time" => {
                change.time = Some(value.parse().unwrap());
            }
            "user" => {
                change.user = Some(value.to_string());
            }
            "desc" => {
                change.description = Some(value.to_string());
            }
            _ => {}
        };
    }
}

impl<ReadT: io::Read> Iterator for P4ChangesIterator<ReadT> {
    type Item = P4Changelist;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(kvp) = self.parser.get_next_kvp().unwrap() {
            if Some(kvp.dict_index) != self.previous_dict_index {
                // We are done with the current record, so we can store it
                let change = std::mem::take(&mut self.current_change).try_into().unwrap();
                self.previous_dict_index = Some(kvp.dict_index);

                Self::populate_field(&mut self.current_change, kvp.key, kvp.value);

                return Some(change);
            }

            Self::populate_field(&mut self.current_change, kvp.key, kvp.value);
        }

        // Yield the final CL
        if self.previous_dict_index.is_some() {
            let change = std::mem::take(&mut self.current_change).try_into().unwrap();
            self.previous_dict_index = None;
            return Some(change);
        }

        // Ensure the process is cleaned up
        if let Some(mut p4_process) = self.p4_process.take() {
            p4_process.wait().expect("Failed to wait for p4 process");
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_run_changes_cmd() {
        let test_file = fs::File::open("./test_data/changes.pyc").unwrap();
        let mut changes_iter = P4ChangesIterator::new_from_reader(test_file);

        let expected = vec![
            P4Changelist {
                changelist: 10,
                time: 1743724741,
                user: "david".into(),
                description: "Long yeet\n".into(),
                files: vec![],
            },
            P4Changelist {
                changelist: 9,
                time: 1743723360,
                user: "david".into(),
                description: "Description\n".into(),
                files: vec![],
            },
            P4Changelist {
                changelist: 8,
                time: 1743723145,
                user: "david".into(),
                description: "Another description\n".into(),
                files: vec![],
            },
            P4Changelist {
                changelist: 7,
                time: 1743438499,
                user: "david".into(),
                description: "Another change\n".into(),
                files: vec![],
            },
            P4Changelist {
                changelist: 6,
                time: 1743438200,
                user: "david".into(),
                description: "Yo what\n".into(),
                files: vec![],
            },
            P4Changelist {
                changelist: 5,
                time: 1739554022,
                user: "david".into(),
                description: "Test3".into(),
                files: vec![],
            },
            P4Changelist {
                changelist: 2,
                time: 1739476182,
                user: "david".into(),
                description: "Test delete\n".into(),
                files: vec![],
            },
            P4Changelist {
                changelist: 1,
                time: 1739476154,
                user: "david".into(),
                description: "Test submit\n".into(),
                files: vec![],
            },
        ];

        let mut expected_iter = expected.into_iter();
        while let Some(change) = expected_iter.next() {
            assert_eq!(changes_iter.next(), Some(change), "Change mismatch");
        }

        // Make sure there are no more records
        assert_eq!(changes_iter.next(), None, "Expected no more changes");
    }
}
