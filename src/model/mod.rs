pub mod update_options;
pub mod version;
pub use version::Version;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Group {
    pub value: String,
}

impl Group {
    pub fn new(name: &str) -> Self {
        Group {
            value: name.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Artifact {
    pub value: String,
}

impl Artifact {
    pub fn new(name: &str) -> Self {
        Artifact {
            value: name.to_string(),
        }
    }
}
