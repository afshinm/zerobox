use std::path::PathBuf;

use zerobox_utils_absolute_path::AbsolutePathBuf;

pub fn find_home() -> std::io::Result<AbsolutePathBuf> {
    let env = std::env::var("ZEROBOX_HOME").ok().filter(|v| !v.is_empty());
    let path = match env {
        Some(val) => {
            let path = PathBuf::from(&val);
            if !path.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("ZEROBOX_HOME {val:?} does not exist"),
                ));
            }
            path.canonicalize()?
        }
        None => {
            let mut p = dirs::home_dir().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "no home directory")
            })?;
            p.push(".zerobox");
            p
        }
    };
    AbsolutePathBuf::from_absolute_path(path)
}
