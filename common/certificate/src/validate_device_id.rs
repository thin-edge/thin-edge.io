use crate::CertificateError;

fn is_valid_device_id_char(ch: char) -> bool {
    matches!(ch, 'A'..='Z'
    | 'a'..='z'
    | '0'..='9'
    | '\''
    | '='
    | '('
    | ')'
    | '+'
    | ','
    | '-'
    | '.'
    | ':'
    | '?')
}

pub fn is_valid_device_id(id: &str, max_cn_size: usize) -> Result<(), CertificateError> {
    if id.is_empty() {
        return Err(CertificateError::EmptyName);
    } else if id.len() > max_cn_size {
        return Err(CertificateError::TooLongName {
            name: id.into(),
            max_cn_size,
        });
    } else if !id.chars().all(is_valid_device_id_char) || id.contains(char::is_control) {
        return Err(CertificateError::InvalidCharacter { name: id.into() });
    }

    Ok(())
}
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn valid_device_id_with_aplhabets() {
        assert!("ThinEdgeDevice".chars().all(is_valid_device_id_char));
    }

    #[test]
    fn valid_device_id_with_digits() {
        assert!("123ThinEdgeDevice".chars().all(is_valid_device_id_char));
    }
    #[test]
    fn valid_device_id_with_special_chars() {
        assert!("'?:=()+,-.:123ThinEdgeDevice-id"
            .chars()
            .all(is_valid_device_id_char));
    }
    #[test]
    fn invalid_device_ids() {
        for id in vec![
            "thinedge>",
            "thinedge<",
            "thinedge@",
            "thinedge#",
            "thinedge|",
            "thinedge[",
            "thinedge]",
            "thinedge}",
            "thinedge{",
            "thinedge^",
            "thinedge*",
            "thinedge&",
            "thin_edge",
            "thin edge",
        ]
        .iter()
        {
            assert_eq!(id.chars().all(is_valid_device_id_char), false);
        }
    }
}
