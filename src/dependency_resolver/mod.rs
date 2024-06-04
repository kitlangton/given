use crate::{
    model::{Artifact, Group, Version},
    parser::{get_deps, get_scala_version_from_build_sbt, span::Edit, Span, WithSpan},
};
use anyhow::{Context, Result};
use std::{collections::HashMap, path::Path};
use std::{fs, path::PathBuf};

#[derive(Debug)]
pub struct Dependency {
    pub group: Group,
    pub artifact: Artifact,
    pub version: WithSpan<Version>,
}

#[derive(Debug)]
pub struct FileDependencies {
    pub file_path: PathBuf,
    pub dependencies: Vec<(Group, Artifact, WithSpan<Version>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub file_path: PathBuf,
    pub span: Span,
}

// A particular group and artifact might exist in the codebase at MULTIPLE locations.
// These should be grouped together.
#[derive(Debug, Clone)]
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

    pub fn add_dependency(&mut self, dependency: Dependency, location: Location) {
        let key = (dependency.group, dependency.artifact);
        self.map
            .entry(key)
            .and_modify(|existing| existing.add(&dependency.version.value, &location))
            .or_insert_with(|| VersionWithLocations::new(&dependency.version.value, &location));
    }

    pub fn add_file_dependencies(&mut self, file_dependencies: FileDependencies) {
        for (group, artifact, version) in &file_dependencies.dependencies {
            self.add_dependency(
                Dependency {
                    group: group.clone(),
                    artifact: artifact.clone(),
                    version: version.clone(),
                },
                Location {
                    file_path: file_dependencies.file_path.clone(),
                    span: version.position.clone(),
                },
            );
        }
    }

    pub fn from_file_dependencies(file_dependencies: Vec<FileDependencies>) -> Self {
        let mut map = Self::new();
        for file_dependency in file_dependencies {
            map.add_file_dependencies(file_dependency);
        }
        map
    }
}

/// - read build.sbt
/// - read every scala file in the project folder
/// TODO: Support val defs defined in OTHER files.
pub fn collect_sbt_dependencies(project_path: &Path) -> Result<DependencyMap> {
    // 1. Load dependencies from build.sbt
    let build_sbt_path = project_path.join("build.sbt");
    let mut dependencies = Vec::new();

    if build_sbt_path.exists() {
        let project_dependencies = collect_dependencies_from_file(build_sbt_path.as_path())
            .context("Failed to collect dependencies from build.sbt")?;
        dependencies.push(project_dependencies);

        let code =
            fs::read_to_string(build_sbt_path.as_path()).context("Failed to read build.sbt")?;

        if let Some(scala_version) = get_scala_version_from_build_sbt(&code) {
            dependencies.push(FileDependencies {
                file_path: build_sbt_path,
                dependencies: vec![scala_version],
            });
        }
    }

    // 2. Load dependencies from repo/project/plugins.sbt

    let plugins_sbt_path = project_path.join("project/plugins.sbt");
    if plugins_sbt_path.exists() {
        let plugins_sbt_dependencies =
            collect_dependencies_from_file(plugins_sbt_path.as_path())
                .context("Failed to collect dependencies from plugins.sbt")?;
        dependencies.push(plugins_sbt_dependencies);
    }

    // 3. Load dependencies from repo/project/
    let project_folder = project_path.join("project");
    if project_folder.exists() {
        let scala_files = collect_dependencies_from_dir(project_folder, Some("scala"))
            .context("Failed to collect dependencies from Scala files")?;
        dependencies.extend(scala_files);
    }

    Ok(DependencyMap::from_file_dependencies(dependencies))
}

pub fn collect_dependencies_from_dir(
    dir: PathBuf,
    extension_filter: Option<&str>,
) -> Result<Vec<FileDependencies>> {
    let mut results = Vec::new();
    let paths = fs::read_dir(dir).context("Failed to read directory")?;

    for path in paths {
        let path = path.context("Failed to read path")?.path();
        if path.is_file() {
            if let Some(ext) = extension_filter {
                if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                    continue;
                }
            }

            let file_path_str = path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 path"))?;

            let file_dependencies = collect_dependencies_from_file(Path::new(file_path_str))
                .context(format!(
                    "Failed to collect dependencies from file: {}",
                    file_path_str
                ))?;

            results.push(file_dependencies);
        }
    }

    Ok(results)
}

pub fn collect_dependencies_from_file(file_path: &Path) -> Result<FileDependencies> {
    let code = fs::read_to_string(file_path)
        .context(format!("Failed to read file: {}", file_path.display()))?;
    let dependencies = get_deps(&code);

    Ok(FileDependencies {
        file_path: PathBuf::from(file_path),
        dependencies,
    })
}

pub fn write_version_updates(updates: Vec<(Version, Vec<Location>)>) -> std::io::Result<()> {
    use std::collections::HashMap;
    use std::fs;

    // Step 1: Group updates by file path
    let mut updates_by_file: HashMap<PathBuf, Vec<Edit>> = HashMap::new();
    for (version, locations) in updates {
        for location in locations {
            let edit = Edit {
                span: location.span.clone(),
                text: format!("\"{}\"", version),
            };
            updates_by_file
                .entry(location.file_path.clone())
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
        // Step 1: Create a temporary directory and sample build.sbt file
        println!("Creating temporary directory and sample build.sbt file...");
        let dir = tempdir()?;
        let file_path = dir.path().join("build.sbt");
        let mut file = File::create(&file_path)?;
        writeln!(
            file,
            r#"
            libraryDependencies ++= Seq(
              "dev.zio" %% "zio" % "2.0.0",
              "org.postgresql" % "postgresql" % "42.5.1"
            )
        "#
        )?;
        println!("Sample build.sbt file created at {:?}", file_path);

        // Step 2: Read the dependencies from the file
        println!("Reading dependencies from the file...");
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

        // Step 4: Write the updated dependencies back to the file
        println!("Writing updated dependencies back to the file...");
        write_version_updates(updates)?;
        println!("Updated dependencies written to the file.");

        // Verify the updates
        println!("Verifying the updates...");
        let updated_content = fs::read_to_string(&file_path)?;
        println!("Updated content: {}", updated_content);
        let expected_content = r#"
            libraryDependencies ++= Seq(
              "dev.zio" %% "zio" % "999.999.999",
              "org.postgresql" % "postgresql" % "999.999.999"
            )
        "#;
        assert_eq!(updated_content.trim(), expected_content.trim());
        println!("Updates verified successfully.");

        Ok(())
    }
}
