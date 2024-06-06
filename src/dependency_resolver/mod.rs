use crate::{
    model::{Artifact, Group, Version},
    parser::{get_scala_version_from_build_sbt, span::Edit, Dependency, DependencyParser, Span},
};
use anyhow::Result;
use std::{collections::HashMap, path::Path};
use std::{fs, path::PathBuf};

mod file_cache;

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    pub span: Span,
}

impl Location {
    pub fn new(path: PathBuf, span: Span) -> Self {
        Self { path, span }
    }
}

// A particular group and artifact might exist in the codebase at MULTIPLE locations.
// These should be grouped together.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionWithLocations {
    pub version: Version,
    pub locations: Vec<Location>,
}

impl VersionWithLocations {
    pub fn new(version: &Version, location: &Location) -> Self {
        Self {
            version: version.clone(),
            locations: vec![location.clone()],
        }
    }

    /// Add version with location, it should take the greater version + concat the locations
    pub fn add(&mut self, version: &Version, location: &Location) {
        if version > &self.version {
            self.version = version.clone();
        }
        self.locations.push(location.clone());
    }
}

#[derive(Debug)]
pub struct DependencyMap {
    map: HashMap<(Group, Artifact), VersionWithLocations>,
}

impl DependencyMap {
    pub fn iter(
        &self,
    ) -> std::collections::hash_map::Iter<(Group, Artifact), VersionWithLocations> {
        self.map.iter()
    }
}

impl IntoIterator for DependencyMap {
    type Item = ((Group, Artifact), VersionWithLocations);
    type IntoIter = std::collections::hash_map::IntoIter<(Group, Artifact), VersionWithLocations>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter()
    }
}

impl Default for DependencyMap {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyMap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn from_dependencies(dependencies: Vec<Dependency>) -> Self {
        let mut map = Self::new();
        for dependency in dependencies {
            map.add_dependency(&dependency);
        }
        map
    }

    pub fn add_dependency(&mut self, dependency: &Dependency) {
        let key = (dependency.group.clone(), dependency.artifact.clone());
        let location = &dependency.version.location;
        self.map
            .entry(key)
            .and_modify(|existing| existing.add(&dependency.version.value, location))
            .or_insert_with(|| VersionWithLocations::new(&dependency.version.value, location));
    }
}

/// - read build.sbt
/// - read every scala file in the project folder
pub fn collect_sbt_dependencies(project_path: &Path) -> Result<DependencyMap> {
    let mut dependency_parser = DependencyParser::new();
    let all_dependency_paths = all_dependency_paths(project_path);
    let mut file_cache = file_cache::FileCache::new();

    // load all val defs from all files
    for path in &all_dependency_paths {
        let code = file_cache.read_to_string(path)?;
        dependency_parser.parse_val_defs(path, &code);
    }

    // load all dependencies from all files
    for path in &all_dependency_paths {
        let code = file_cache.read_to_string(path)?;
        dependency_parser.parse_dependencies(path, &code);
    }

    let mut dependencies = dependency_parser.dependencies;

    // attempt to parse scala version from build.sbt
    let build_sbt_path = project_path.join("build.sbt");
    if build_sbt_path.exists() {
        let code = file_cache.read_to_string(&build_sbt_path)?;
        if let Some(scala_version) = get_scala_version_from_build_sbt(&build_sbt_path, &code) {
            dependencies.push(scala_version);
        }
    }

    Ok(DependencyMap::from_dependencies(dependencies))
}

pub fn write_version_updates(updates: &[(Version, Vec<Location>)]) -> std::io::Result<()> {
    // Step 1: Group updates by file path
    let mut updates_by_file: HashMap<PathBuf, Vec<Edit>> = HashMap::new();
    for (version, locations) in updates {
        for location in locations {
            let edit = Edit {
                span: location.span.clone(),
                text: format!("\"{}\"", version),
            };
            updates_by_file
                .entry(location.path.clone())
                .or_default()
                .push(edit);
        }
    }

    // Step 2: Apply edits and write back to files
    for (file_path, edits) in updates_by_file {
        let original_content = fs::read_to_string(&file_path)?;
        let updated_content = Edit::apply_edits(edits, &original_content);
        fs::write(file_path, updated_content)?;
    }

    Ok(())
}

fn all_dependency_paths(project_path: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        project_path.join("build.sbt"),
        project_path.join("project/plugins.sbt"),
    ];

    collect_scala_files(project_path, &mut paths);
    paths.into_iter().filter(|path| path.exists()).collect()
}

fn collect_scala_files(dir: &Path, paths: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_scala_files(&path, paths);
            } else if path.extension().and_then(|e| e.to_str()) == Some("scala") {
                paths.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_collect_dependencies_from_dir() {
        let path = Path::new("/Users/kit/code/archive/scala-update-2/");
        let result = collect_sbt_dependencies(&path);
        if let Ok(deps) = result {
            for (_, dep) in deps.map.iter() {
                println!("{:?}", dep);
            }
        } else {
            println!("Error: {:?}", result);
        }
    }

    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_full_stack_version_update() -> std::io::Result<()> {
        // Step 1: Create a temporary directory and sample build.sbt and versions.scala files
        println!("Creating temporary directory and sample build.sbt and versions.scala files...");
        let dir = tempdir()?;
        let build_sbt_path = dir.path().join("build.sbt");
        let versions_scala_path = dir.path().join("project/versions.scala");

        // Create build.sbt file
        let mut build_sbt_file = File::create(&build_sbt_path)?;
        writeln!(
            build_sbt_file,
            r#"
import Versions._
libraryDependencies ++= Seq(
    "dev.zio" %% "zio" % zio,
    "io.github.kitlangton" %% "neotype" % Versions.neotype,
    "org.postgresql" % "postgresql" % "42.5.1"
)
        "#
        )?;
        println!("Sample build.sbt file created at {:?}", build_sbt_path);

        // Create versions.scala file
        fs::create_dir_all(versions_scala_path.parent().unwrap())?;
        let mut versions_scala_file = File::create(&versions_scala_path)?;
        writeln!(
            versions_scala_file,
            r#"
object Versions {{
    val zio = "2.0.0"
    val neotype = "0.1.0"
}}
            "#
        )?;
        println!(
            "Sample versions.scala file created at {:?}",
            versions_scala_path
        );

        // Step 2: Read the dependencies from the files
        println!("Reading dependencies from the files...");
        let dependencies = collect_sbt_dependencies(&dir.path());
        println!("Dependencies read: {:?}", dependencies);

        // Step 3: Select new versions for the dependencies
        println!("Selecting new versions for the dependencies...");
        let updates: Vec<(Version, Vec<Location>)> = dependencies
            .unwrap()
            .map
            .iter()
            .map(|(_, dep)| (Version::new("999.999.999"), dep.locations.clone()))
            .collect();
        println!("Updates selected: {:?}", updates);

        // Step 4: Write the updated dependencies back to the files
        println!("Writing updated dependencies back to the files...");
        write_version_updates(&updates)?;
        println!("Updated dependencies written to the files.");

        // Verify the updates
        println!("Verifying the updates...");
        let updated_build_sbt_content = fs::read_to_string(&build_sbt_path)?;
        let updated_versions_scala_content = fs::read_to_string(&versions_scala_path)?;
        println!("Updated build.sbt content: {}", updated_build_sbt_content);
        println!(
            "Updated versions.scala content: {}",
            updated_versions_scala_content
        );

        let expected_build_sbt_content = r#"
import Versions._
libraryDependencies ++= Seq(
    "dev.zio" %% "zio" % zio,
    "io.github.kitlangton" %% "neotype" % Versions.neotype,
    "org.postgresql" % "postgresql" % "999.999.999"
)
        "#;
        let expected_versions_scala_content = r#"
object Versions {
    val zio = "999.999.999"
    val neotype = "999.999.999"
}
        "#;

        assert_eq!(
            updated_build_sbt_content.trim(),
            expected_build_sbt_content.trim()
        );
        assert_eq!(
            updated_versions_scala_content.trim(),
            expected_versions_scala_content.trim()
        );
        println!("Updates verified successfully.");

        Ok(())
    }
}
