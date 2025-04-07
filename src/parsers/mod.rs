pub mod py_dict;

#[derive(Debug)]
pub struct P4KeyValuePair<'a> {
    pub dict_index: u32,
    pub key: &'a str,
    pub value: &'a str,
}

pub trait P4KvpStream<ErrorT : std::error::Error> {
    fn get_next_kvp<'b>(&'b mut self) -> Result<Option<P4KeyValuePair<'b>>, ErrorT>;
}
