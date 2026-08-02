use std::ffi::OsString;

#[must_use]
pub fn masked_command(arguments: &[OsString]) -> String {
    let mut output = Vec::with_capacity(arguments.len());
    let mut mask_next = false;
    for argument in arguments {
        let argument = argument.to_string_lossy();
        if mask_next {
            output.push("••••".to_owned());
            mask_next = false;
            continue;
        }
        let lower = argument.to_ascii_lowercase();
        if is_secret_key(&lower) {
            output.push(quote(&argument));
            mask_next = true;
        } else if let Some((key, _value)) = argument.split_once('=')
            && is_secret_key(&key.to_ascii_lowercase())
        {
            output.push(format!("{key}=••••"));
        } else if lower.starts_with("bearer ") || lower.starts_with("basic ") {
            output.push("••••".to_owned());
        } else {
            output.push(quote(&argument));
        }
    }
    output.join(" ")
}

fn is_secret_key(value: &str) -> bool {
    let normalized = value.trim_start_matches('-').replace('_', "-");
    matches!(
        normalized.as_str(),
        "password"
            | "passwd"
            | "token"
            | "access-token"
            | "refresh-token"
            | "api-key"
            | "apikey"
            | "secret"
            | "client-secret"
            | "authorization"
            | "auth"
    )
}

fn quote(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-._/:@".contains(character))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::masked_command;

    #[test]
    fn masks_separate_and_inline_secrets() {
        let command = [
            OsString::from("app"),
            OsString::from("--token"),
            OsString::from("very-secret"),
            OsString::from("--password=hunter2"),
            OsString::from("visible value"),
        ];

        let masked = masked_command(&command);

        assert!(!masked.contains("very-secret"));
        assert!(!masked.contains("hunter2"));
        assert!(masked.contains("--token ••••"));
        assert!(masked.contains("--password=••••"));
        assert!(masked.contains("'visible value'"));
    }
}
