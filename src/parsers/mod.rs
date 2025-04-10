pub mod py_dict;
pub mod ztag;

#[derive(Debug, PartialEq)]
pub struct P4KeyValuePair<'a> {
    pub dict_index: u32,
    pub key: &'a str,
    pub value: &'a str,
}

pub trait P4KvpStream<ErrorT: std::error::Error> {
    fn get_next_kvp<'b>(&'b mut self) -> Result<Option<P4KeyValuePair<'b>>, ErrorT>;
}

#[cfg(test)]
mod tests {
    use super::{py_dict::*, ztag::*};
    use std::fs;

    // Validate the the output from the two parsers is the same
    #[test]
    fn validate_parsers() {
        let mut reader_dict = fs::File::open("./test_data/changes.pyc").unwrap();
        let mut parser_dict = P4PyDictParser::new(&mut reader_dict);

        let mut reader_ztag = fs::File::open("./test_data/changes.ztag").unwrap();
        let mut parser_ztag = P4ZtagParser::new(&mut reader_ztag, Some("change"));

        let mut record_count = 0;
        while let Some(mut kvp_dict) = parser_dict.get_next_kvp().unwrap() {
            let kvp_ztag = parser_ztag.get_next_kvp().unwrap().unwrap();

            // The output of the python dict parser starts each record with a "code" field, so skip that
            if kvp_dict.key == "code" {
                kvp_dict = parser_dict.get_next_kvp().unwrap().unwrap();
            }
            assert_eq!(
                kvp_dict.dict_index, kvp_ztag.dict_index,
                "Dict index mismatch on {:?} vs {:?}",
                kvp_dict, kvp_ztag
            );
            assert_eq!(
                kvp_dict.key, kvp_ztag.key,
                "Dict key mismatch on {:?} vs {:?}",
                kvp_dict, kvp_ztag
            );
            assert_eq!(
                kvp_dict.value.trim_ascii_end(),
                kvp_ztag.value.trim_ascii_end(),
                "Dict value (trimmed) mismatch on {:?} vs {:?}",
                kvp_dict,
                kvp_ztag
            );

            record_count += 1;
        }

        assert_eq!(
            record_count, 64,
            "Should have 64 records, but got {}",
            record_count
        );

        // Make sure both iterators are exhausted
        assert!(
            parser_dict.get_next_kvp().unwrap().is_none(),
            "Dict parser should be exhausted"
        );
        assert!(
            parser_ztag.get_next_kvp().unwrap().is_none(),
            "Ztag parser should be exhausted"
        );
    }
}
