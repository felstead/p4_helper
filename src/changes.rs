// == Std crates
use std::ops::Range;

// == Internal crates
use crate::*;
use crate::parsers::py_dict::*;

pub struct P4ChangesIterator {
    p4_process: process::Child,
    parser: P4PyDictParser<process::ChildStdout>,
    // Storage for various state variables
    previous_dict_index: Option<u32>,
    current_change: InterimP4Changelist,
}

impl P4ChangesIterator {
    pub fn new(cl_range: Option<Range<u32>>) -> Result<Self, &'static str> {
        let cl_range = cl_range.unwrap_or(0..u32::MAX);

        let mut p4_process = get_p4_cmd(vec!["changes", "-s", "submitted", &format!("@{},{}", cl_range.start, cl_range.end)])
            .spawn()
            .expect("Failed to start p4 command");

        let reader = p4_process.stdout.take().expect("Failed to get stdout of p4 command");
        let parser = P4PyDictParser::new(reader);

        Ok(P4ChangesIterator {
            p4_process,
            parser,
            previous_dict_index: Some(0),
            current_change: InterimP4Changelist::default(),
        })
    }

    fn populate_field(change : &mut InterimP4Changelist, key: &str, value: &str) {
        match key {
            "change" => { change.change = Some(value.parse().unwrap()); },
            "time" => { change.time = Some(value.parse().unwrap()); },
            "user" => { change.user = Some(value.to_string()); },
            "desc" => { change.description = Some(value.to_string()); },
            _ => { }
        };
    }
}

impl Iterator for P4ChangesIterator {
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
        self.p4_process.wait().unwrap();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_changes_cmd() {
        let mut changes_iterator = P4ChangesIterator::new(None).unwrap();

        while let Some(change) = changes_iterator.next() {
            println!("{:?}", change);
        }

    }
}