pub mod maven;

use crate::model::{Artifact, Group, Version};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait PackageSearch {
    async fn search_artifacts(&self, group: &Group, artifact_prefix: &str)
        -> Result<Vec<Artifact>>;

    async fn get_versions(&self, group: &Group, artifact: &Artifact) -> Result<Vec<Version>>;
}

#[async_trait]
pub trait PackageSearchExt: PackageSearch {
    /// given the group and artifact prefix, find the first suffix (_2.13, _3) that returns a non-empty list of versions
    async fn find_first_suffix(
        &self,
        group: &Group,
        artifact: &Artifact,
        suffixes: Vec<&str>,
    ) -> Result<Vec<Artifact>> {
        for suffix in suffixes {
            let artifact_prefix = format!("{}{}", artifact.value, suffix);
            let artifacts = self.search_artifacts(group, &artifact_prefix).await?;
            if !artifacts.is_empty() {
                return Ok(artifacts);
            }
        }
        Ok(vec![])
    }

    async fn get_firsts_with_suffix(
        &self,
        group: &Group,
        artifact: &Artifact,
        suffixes: Vec<&str>,
    ) -> Result<Vec<Version>> {
        let artifacts = self.find_first_suffix(group, artifact, suffixes).await?;
        if let Some(first_artifact) = artifacts.first() {
            let versions = self.get_versions(group, first_artifact).await?;
            Ok(versions)
        } else {
            Ok(vec![])
        }
    }

    async fn get_multiple_versions(
        &self,
        group_artifact_pairs: Vec<(Group, Artifact)>,
        maybe_scala_version: Option<Version>,
    ) -> Result<HashMap<(Group, Artifact), Vec<Version>>> {
        let suffixes = match maybe_scala_version {
            Some(scala_version) if scala_version.major() == Some(3) => {
                vec!["_3", "_2.13", "_2.12", ""]
            }
            Some(scala_version)
                if scala_version.major() == Some(2) && scala_version.minor() == Some(13) =>
            {
                vec!["_2.13", "_2.12", ""]
            }
            Some(scala_version)
                if scala_version.major() == Some(2) && scala_version.minor() == Some(12) =>
            {
                vec!["_2.12", ""]
            }
            _ => vec!["_2.13", "_3", "_2.12", ""],
        };

        let futures = group_artifact_pairs.into_iter().map(|(group, artifact)| {
            let group_clone = group.clone();
            let artifact_clone = artifact.clone();
            let suffixes_clone = suffixes.clone();
            async move {
                let versions = self
                    .get_firsts_with_suffix(&group, &artifact, suffixes_clone)
                    .await;
                ((group_clone, artifact_clone), versions)
            }
        });

        let results = futures::future::join_all(futures).await;

        let mut versions_map = HashMap::new();
        for ((group, artifact), versions_result) in results {
            if let Ok(versions) = versions_result {
                versions_map.insert((group, artifact), versions);
            }
        }

        Ok(versions_map)
    }
}

impl<T: PackageSearch + Sync> PackageSearchExt for T {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::dependency_resolver;
    use crate::model::{Artifact, Group};
    use crate::package_search::maven::MavenPackageSearch;
    use anyhow::Result;
    use tokio;

    #[tokio::test]
    async fn test_get_multiple_versions_for_dev_zio_libraries() -> Result<()> {
        let maven_search = MavenPackageSearch::new();

        let group_artifact_pairs = vec![
            (Group::new("dev.zio"), Artifact::new("zio")),
            (Group::new("dev.zio"), Artifact::new("zio-json")),
            (Group::new("dev.zio"), Artifact::new("zio-schema")),
        ];

        let versions_map = maven_search
            .get_multiple_versions(group_artifact_pairs, None)
            .await?;

        assert_eq!(versions_map.len(), 3);
        for ((group, artifact), version_list) in versions_map.iter() {
            if version_list.is_empty() {
                eprintln!(
                    "Warning: Version list for {:?}/{:?} is empty",
                    group, artifact
                );
            }
        }

        println!("Versions retrieved: {:?}", versions_map);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_versions_from_loaded_dependencies() -> Result<()> {
        let dependencies = dependency_resolver::collect_sbt_dependencies(Path::new(
            "/Users/kit/code/archive/scala-update-2/",
        ))?;

        let all_groups_and_artifacts = dependencies
            .iter()
            .map(|(dep, _)| (dep.0.clone(), Artifact::new(&format!("{}_3", dep.1.value))))
            .collect::<Vec<_>>();

        let maven_search = MavenPackageSearch::new();
        let versions_map = maven_search
            .get_multiple_versions(all_groups_and_artifacts, None)
            .await?;

        assert!(!versions_map.is_empty(), "Versions map should not be empty");
        for ((group, artifact), version_list) in versions_map.iter() {
            if version_list.is_empty() {
                eprintln!(
                    "Warning: Version list for {:?}/{:?} is empty",
                    group, artifact
                );
            }
        }

        println!("Versions retrieved: {:?}", versions_map);

        Ok(())
    }
}
