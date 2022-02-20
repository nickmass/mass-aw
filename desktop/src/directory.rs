use engine::error::Error;
use engine::resources::Io;

pub struct DirectoryIo {
    base_path: std::path::PathBuf,
}

impl DirectoryIo {
    pub fn new<P: Into<std::path::PathBuf>>(base_path: P) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }
}

impl Io for DirectoryIo {
    type Reader = std::fs::File;

    fn load<S: AsRef<str>>(&self, name: S) -> Result<Self::Reader, Error> {
        let path = self.base_path.join(name.as_ref());

        eprintln!("load: {}", path.display());

        Ok(std::fs::File::open(path)?)
    }
}
