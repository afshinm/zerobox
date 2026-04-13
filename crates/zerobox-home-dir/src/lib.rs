use std::path::PathBuf;

pub fn find_home() -> std::io::Result<PathBuf> {
    let env = std::env::var("ZEROBOX_HOME").ok().filter(|v| !v.is_empty());
    match env {
        Some(val) => {
            let path = PathBuf::from(&val);
            if !path.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("ZEROBOX_HOME {val:?} does not exist"),
                ));
            }
            path.canonicalize()
        }
        None => {
            let mut p = dirs::home_dir().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "no home directory")
            })?;
            p.push(".zerobox");
            Ok(p)
        }
    }
}
