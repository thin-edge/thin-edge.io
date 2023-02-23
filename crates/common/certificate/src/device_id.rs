fn is_valid_device_id_char(ch: char) -> bool {
    matches!(ch, 'A'..='Z'
    | 'a'..='z'
    | '0'..='9'
    | '\''
    | '='
    | '('
    | ')'
    | ','
    | '-'
    | '.'
    | '?'
    | '_'
    | '*'
    | '@'
    | '!'
    | '%')
}

pub fn is_valid_device_id(id: &str, max_cn_size: usize) -> Result<(), DeviceIdError> {
    if id.is_empty() {
        return Err(DeviceIdError::EmptyName);
    } else if id.len() > max_cn_size {
        return Err(DeviceIdError::TooLongName {
            name: id.into(),
            max_cn_size,
        });
    } else if !id.chars().all(is_valid_device_id_char) {
        return Err(DeviceIdError::InvalidCharacter { name: id.into() });
    }

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum DeviceIdError {
    #[error(r#"The string '{name:?}' contains characters which cannot be used in a name [use only A-Z, a-z, 0-9, ' = ( ) , - . ? % * _ ! @]"#)]
    InvalidCharacter { name: String },

    #[error(r#"The empty string cannot be used as a name"#)]
    EmptyName,

    #[error(
    r#"The string '{name:?}' is more than {max_cn_size} characters long and cannot be used as a name"#
    )]
    TooLongName { name: String, max_cn_size: usize },
}

#[cfg(test)]
mod test {
    use super::*;
    use assert_matches::*;

    #[test]
    fn valid_device_id_with_alphabets() {
        assert!(is_valid_device_id("ThinEdgeDevice", 64).is_ok());
    }

    #[test]
    fn valid_device_id_with_digits() {
        assert!(is_valid_device_id("123ThinEdgeDevice", 64).is_ok());
    }
    #[test]
    fn valid_device_id_with_special_chars() {
        assert!(is_valid_device_id("'?=()*@!%,-.123ThinEdgeDevice-id", 64).is_ok());
    }

    #[test]
    fn invalid_device_ids() -> anyhow::Result<()> {
        for id in vec![
            "thinedge>",
            "thinedge<",
            "thinedge|",
            "thinedge[",
            "thinedge]",
            "thinedge}",
            "thinedge{",
            "thinedge^",
            "thinedge&",
            "thinedge#",
            "thinedge$",
            "thinedge+",
            "thin/edge",
            "thin:edge",
            "thin$edge",
            "thin edge",
        ]
        .iter()
        {
            let output_err = is_valid_device_id(id, 64);
            assert_matches!(
                output_err,
                Err(DeviceIdError::InvalidCharacter { ref name }) if name == id
            );
        }
        Ok(())
    }

    #[test]
    fn empty_device_id() -> anyhow::Result<()> {
        let output_err = is_valid_device_id("", 64);
        assert_matches!(output_err, Err(DeviceIdError::EmptyName));

        Ok(())
    }

    #[test]
    fn too_long_device_id() -> anyhow::Result<()> {
        let id = "thinedgethinedgethinedgethinedgethinedgethinedgethinedgethinedgethinedge";
        let output_err = is_valid_device_id(id, 64);
        assert_matches!(
            output_err,
            Err(DeviceIdError::TooLongName {
                name,
                max_cn_size: 64
            }) if name == id
        );

        Ok(())
    }
}
