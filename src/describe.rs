// == Std crates
use std::{io, process};

// == Internal crates
use crate::parsers::py_dict::P4PyDictParser;
use crate::*;

pub struct P4DescribeIterator<ReadT: io::Read> {
    p4_process: Option<process::Child>,
    parser: P4PyDictParser<ReadT>,
    changelist: P4Changelist,
    // Storage for various state variables
    current_file_index: Option<u32>,
    current_file: InterimP4File,
}

impl<ReadT: io::Read> P4DescribeIterator<ReadT> {
    pub fn new(changelist: u32) -> Result<P4DescribeIterator<process::ChildStdout>, &'static str> {
        let mut p4_process = get_p4_cmd(vec!["describe", "-s", &format!("{}", changelist)])
            .spawn()
            .expect("Failed to start p4 command");

        let reader = p4_process
            .stdout
            .take()
            .expect("Failed to get stdout of p4 command");

        let mut result = P4DescribeIterator::<process::ChildStdout>::new_from_reader(reader)?;
        result.p4_process = Some(p4_process);

        Ok(result)
    }

    pub fn new_from_reader(reader: ReadT) -> Result<Self, &'static str> {
        let mut parser = P4PyDictParser::new(reader);

        let mut current_file_index = None;
        let mut current_change = InterimP4Changelist::default();
        let mut current_file = InterimP4File::default();

        // Read the first parts to get the CL information
        while let Some(kvp) = parser.get_next_kvp().unwrap() {
            match kvp.key {
                "change" => {
                    current_change.change = Some(kvp.value.parse().unwrap());
                }
                "time" => {
                    current_change.time = Some(kvp.value.parse().unwrap());
                }
                "user" => {
                    current_change.user = Some(kvp.value.to_string());
                }
                "desc" => {
                    current_change.description = Some(kvp.value.to_string());
                }
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
            p4_process: None,
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
            "depotFile" => {
                file.depot_path = Some(value.to_string());
            }
            "action" => {
                file.action = Some(value.to_string());
            }
            "rev" => {
                file.revision = Some(value.parse().unwrap());
            }
            "fileSize" => {
                file.file_size = Some(value.parse().unwrap());
            }
            "digest" => file.digest = Some(const_hex::decode_to_array(value).unwrap()),
            _ => {} // No op
        }
    }
}

impl<ReadT: io::Read> Iterator for P4DescribeIterator<ReadT> {
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

        if let Some(mut p4_process) = self.p4_process.take() {
            // Wait for the p4 process to finish
            p4_process.wait().unwrap();
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_describe() {
        let input_file = fs::File::open("./test_data/describe.pyc").unwrap();
        let mut describe_iter = P4DescribeIterator::new_from_reader(input_file).unwrap();

        let expected = vec![
            P4File { depot_path: "//depot/main3/UE5.5_github_src/.editorconfig".into(), action: "add".into(), revision: 1, file_size: 1015, digest: [169, 233, 51, 32, 225, 252, 70, 146, 40, 215, 7, 201, 18, 76, 135, 140] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/.gitattributes".into(), action: "add".into(), revision: 1, file_size: 522, digest: [172, 90, 50, 21, 100, 157, 221, 71, 43, 74, 89, 74, 35, 198, 32, 190] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/.gitignore".into(), action: "add".into(), revision: 1, file_size: 8396, digest: [158, 21, 68, 189, 138, 27, 249, 134, 129, 107, 108, 15, 200, 3, 246, 82] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/Default.uprojectdirs".into(), action: "add".into(), revision: 1, file_size: 285, digest: [137, 95, 100, 35, 141, 109, 217, 189, 117, 209, 91, 18, 71, 141, 157, 232] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/Engine/Binaries/DotNET/CsvTools/CSVCollate.deps.json".into(), action: "add".into(), revision: 1, file_size: 1611, digest: [62, 23, 231, 202, 158, 32, 102, 86, 126, 10, 108, 96, 20, 235, 16, 69] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/Engine/Binaries/DotNET/CsvTools/CSVCollate.dll.config".into(), action: "add".into(), revision: 1, file_size: 178, digest: [105, 168, 101, 152, 92, 186, 230, 239, 44, 201, 60, 26, 137, 45, 57, 117] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/Engine/Binaries/DotNET/CsvTools/CSVCollate.runtimeconfig.json".into(), action: "add".into(), revision: 1, file_size: 242, digest: [43, 242, 132, 218, 100, 17, 155, 106, 223, 229, 123, 3, 64, 7, 15, 97] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/Engine/Binaries/DotNET/CsvTools/CsvConvert.deps.json".into(), action: "add".into(), revision: 1, file_size: 1611, digest: [89, 227, 34, 142, 6, 213, 177, 28, 182, 195, 18, 181, 218, 41, 183, 222] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/Engine/Binaries/DotNET/CsvTools/CsvConvert.dll.config".into(), action: "add".into(), revision: 1, file_size: 178, digest: [105, 168, 101, 152, 92, 186, 230, 239, 44, 201, 60, 26, 137, 45, 57, 117] },
            P4File { depot_path: "//depot/main3/UE5.5_github_src/Engine/Binaries/DotNET/CsvTools/CsvConvert.runtimeconfig.json".into(), action: "add".into(), revision: 1, file_size: 242, digest: [43, 242, 132, 218, 100, 17, 155, 106, 223, 229, 123, 3, 64, 7, 15, 97] },
        ];

        let mut expected_iter = expected.into_iter();

        while let Some(expected) = expected_iter.next() {
            assert_eq!(describe_iter.next(), Some(expected));
        }

        // Make sure there are no more records
        assert_eq!(describe_iter.next(), None, "Expected no more files");
    }
}
