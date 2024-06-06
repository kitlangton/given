use anyhow::{Context, Result};

use std::fs;
use std::path::PathBuf;

use std::collections::HashMap;

pub struct FileCache {
    pub(crate) cache: HashMap<PathBuf, String>,
}

impl FileCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn read_to_string(&mut self, path: &PathBuf) -> Result<String> {
        if let Some(content) = self.cache.get(path) {
            Ok(content.clone())
        } else {
            let content = fs::read_to_string(path).context("Failed to read file")?;
            self.cache.insert(path.clone(), content.clone());
            Ok(content)
        }
    }
}
