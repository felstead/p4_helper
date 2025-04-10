pub mod changes;
pub mod describe;
pub mod parsers;

// == Std crates
use std::process;

#[derive(Debug, PartialEq)]
pub struct P4Changelist {
    pub changelist: u32,
    pub time: u32,
    pub user: String,
    pub description: String,
    pub files: Vec<P4File>,
}

#[derive(Debug, PartialEq)]
pub struct P4File {
    pub depot_path: String,
    pub action: String,
    pub revision: u32,
    pub file_size: u64,
    pub digest: [u8; 16],
}

#[derive(Debug, Default)]
struct InterimP4Changelist {
    change: Option<u32>,
    time: Option<u32>,
    user: Option<String>,
    description: Option<String>,
    files: Vec<P4File>,
}

impl TryInto<P4Changelist> for InterimP4Changelist {
    type Error = &'static str;

    fn try_into(self) -> Result<P4Changelist, Self::Error> {
        Ok(P4Changelist {
            changelist: self.change.ok_or("Missing changelist")?,
            time: self.time.ok_or("Missing time")?,
            user: self.user.ok_or("Missing user")?,
            description: self.description.ok_or("Missing description")?,
            files: self.files,
        })
    }
}

#[derive(Debug, Default)]
struct InterimP4File {
    depot_path: Option<String>,
    action: Option<String>,
    revision: Option<u32>,
    file_size: Option<u64>,
    digest: Option<[u8; 16]>,
}

impl TryInto<P4File> for InterimP4File {
    type Error = &'static str;

    fn try_into(self) -> Result<P4File, Self::Error> {
        Ok(P4File {
            depot_path: self.depot_path.ok_or("Missing depot path")?,
            action: self.action.ok_or("Missing action")?,
            revision: self.revision.ok_or("Missing revision")?,
            file_size: self.file_size.ok_or("Missing file size")?,
            digest: self.digest.ok_or("Missing digest")?,
        })
    }
}

// == Utility functions
pub fn get_p4_cmd(args: Vec<&str>) -> process::Command {
    let mut cmd = process::Command::new("p4");
    cmd.args(["-ztag", "-G"])
        .args(args)
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .stdin(process::Stdio::null());
    cmd
}

fn split_indexed_key(key: &str) -> Option<(&str, u32)> {
    if let Some(first_num_index) = key.find(char::is_numeric) {
        Some((
            &key[..first_num_index],
            key[first_num_index..].parse::<u32>().unwrap(),
        ))
    } else {
        None
    }
}
