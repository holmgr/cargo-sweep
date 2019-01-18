use failure::Error;
use serde_derive::{Deserialize, Serialize};
use serde_json::{from_str, to_string};
use std::{
    fs::{remove_file, File},
    io::prelude::*,
    path::Path,
    time::{Duration, SystemTime},
};

/// Serializable system time used for stamp.
#[derive(Serialize, Deserialize, Debug)]
pub struct Timestamp(SystemTime);

impl Timestamp {
    /// Create a new timestamp at the current system time.
    pub fn new() -> Self {
        Timestamp(SystemTime::now())
    }

    /// Attempts to store the timestamp at the given directory.
    pub fn store(&self, target_dir: &Path) -> Result<(), Error> {
        let mut path = target_dir.to_path_buf();
        path.push("sweep.timestamp");
        let mut file = File::create(path)?;
        file.write_all(to_string(&self)?.as_bytes())?;
        Ok(())
    }

    /// Attempts to load the the timestamp file in the given directory.
    pub fn load(target_dir: &Path) -> Result<Timestamp, Error> {
        let mut path = target_dir.to_path_buf();
        path.push("sweep.timestamp");
        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        remove_file(&path)?;
        let timestamp: Timestamp = from_str(&contents)?;
        Ok(timestamp)
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::new()
    }
}

/// Warning: This will return a zero duration if it fails to convert.
impl From<Timestamp> for Duration {
    fn from(timestamp: Timestamp) -> Self {
        timestamp.0.elapsed().unwrap_or(Duration::from_secs(0))
    }
}
