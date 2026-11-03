//! Static asset discovery for the public directory.

use std::fs;
use std::path::{Component, Path};

use crate::manifest::Asset;
use crate::CompileError;

pub(crate) fn discover(public_dir: &Path) -> Result<Vec<Asset>, CompileError> {
    if !public_dir.exists() {
        return Ok(Vec::new());
    }
    if !public_dir.is_dir() {
        return Err(CompileError::MissingDirectory {
            path: public_dir.to_path_buf(),
            kind: "public",
        });
    }

    let mut assets = Vec::new();
    visit(public_dir, public_dir, &mut assets)?;
    assets.sort_by(|left, right| left.url().cmp(right.url()));
    Ok(assets)
}

fn visit(root: &Path, directory: &Path, assets: &mut Vec<Asset>) -> Result<(), CompileError> {
    let entries = fs::read_dir(directory).map_err(|error| crate::error::io(directory, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| crate::error::io(directory, &error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| crate::error::io(&path, &error))?;
        if file_type.is_dir() {
            visit(root, &path, assets)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| CompileError::InvalidAsset {
                    path: path.clone(),
                    message: "the asset is outside the public directory".to_owned(),
                })?
                .to_path_buf();
            let url = asset_url(&relative).map_err(|message| CompileError::InvalidAsset {
                path: path.clone(),
                message,
            })?;
            let bytes = entry
                .metadata()
                .map_err(|error| crate::error::io(&path, &error))?
                .len();
            assets.push(Asset::new(relative, url, bytes));
        }
    }
    Ok(())
}

fn asset_url(relative: &Path) -> Result<String, String> {
    let mut url = String::from("/");
    let mut first = true;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("asset paths must be relative and cannot contain `.` or `..`".to_owned());
        };
        let name = name
            .to_str()
            .ok_or_else(|| "asset file names must be valid UTF-8".to_owned())?;
        if name.chars().any(|character| {
            character == '\\' || character == '?' || character == '#' || character.is_control()
        }) {
            return Err(
                "asset file names cannot contain control characters, `\\`, `?`, or `#`".to_owned(),
            );
        }
        if !first {
            url.push('/');
        }
        first = false;
        encode_segment(name, &mut url);
    }
    Ok(url)
}

fn encode_segment(segment: &str, url: &mut String) {
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            url.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(url, "%{byte:02X}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode_segment;

    #[test]
    fn encodes_unreserved_and_escaped_bytes() {
        let mut url = String::new();
        encode_segment("logo final.svg", &mut url);
        assert_eq!(url, "logo%20final.svg");
        let mut url = String::new();
        encode_segment("café.css", &mut url);
        assert_eq!(url, "caf%C3%A9.css");
    }
}
