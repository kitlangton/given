use crate::model::{Artifact, Group, Version};

use anyhow::{Context, Result};
use async_trait::async_trait;
use quick_xml::de::from_str;
use reqwest::Client;
use serde::Deserialize;

use super::PackageSearch;

#[derive(Deserialize, Debug)]
struct SearchResponse {
    response: SearchResponseBody,
}

#[derive(Deserialize, Debug)]
struct SearchResponseBody {
    docs: Vec<Doc>,
}

#[derive(Deserialize, Debug)]
struct Doc {
    a: String,
}

#[derive(Deserialize, Debug)]
struct Metadata {
    versioning: Versioning,
}

#[derive(Deserialize, Debug)]
struct Versioning {
    versions: Versions,
}

#[derive(Deserialize, Debug)]
struct Versions {
    version: Vec<String>,
}

pub struct MavenPackageSearch {
    client: Client,
}

impl Default for MavenPackageSearch {
    fn default() -> Self {
        MavenPackageSearch::new()
    }
}

impl MavenPackageSearch {
    pub fn new() -> Self {
        MavenPackageSearch {
            client: Client::new(),
        }
    }

    async fn fetch_url(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .header("User-Agent", "Rust reqwest client")
            .send()
            .await
            .context("Failed to send request to Maven Central")?;

        if !response.status().is_success() {
            anyhow::bail!("Request failed with status: {}", response.status());
        }

        response
            .text()
            .await
            .context("Failed to read response body")
    }

    pub async fn get_github_repo(
        &self,
        group: &Group,
        artifact: &Artifact,
        version: &Version,
    ) -> Result<Option<String>> {
        let artifact_search_str = format!("{}_3", artifact.value);
        let search_results = self.search_artifacts(group, &artifact_search_str).await?;
        let first_artifact = search_results
            .first()
            .ok_or_else(|| anyhow::anyhow!("No artifacts found"))?;

        let url = format!(
            "https://repo1.maven.org/maven2/{}/{}/{}/{}-{}.pom",
            group.value.replace('.', "/"),
            first_artifact.value,
            version,
            first_artifact.value,
            version
        );

        let body = self.fetch_url(&url).await?;

        #[derive(Deserialize, Debug)]
        struct Project {
            scm: Option<Scm>,
        }

        #[derive(Deserialize, Debug)]
        struct Scm {
            url: Option<String>,
        }

        let project: Project = from_str(&body).context("Failed to parse POM XML response")?;

        Ok(project.scm.and_then(|scm| scm.url))
    }
}

#[async_trait]
impl PackageSearch for MavenPackageSearch {
    async fn search_artifacts(
        &self,
        group: &Group,
        artifact_prefix: &str,
    ) -> Result<Vec<Artifact>> {
        let query = format!("g:{} AND a:{}*", group.value, artifact_prefix);
        let url = format!(
            "https://search.maven.org/solrsearch/select?q={}&rows=20&wt=json",
            query
        );

        let body = self.fetch_url(&url).await?;

        let search_response: SearchResponse =
            serde_json::from_str(&body).context("Failed to parse JSON response")?;

        Ok(search_response
            .response
            .docs
            .into_iter()
            .map(|doc| Artifact::new(&doc.a))
            .collect())
    }

    async fn get_versions(&self, group: &Group, artifact: &Artifact) -> Result<Vec<Version>> {
        let url = format!(
            "https://repo1.maven.org/maven2/{}/{}/maven-metadata.xml",
            group.value.replace('.', "/"),
            artifact.value
        );

        let body = self.fetch_url(&url).await?;

        let metadata: Metadata = from_str(&body).context("Failed to parse XML response")?;

        Ok(metadata
            .versioning
            .versions
            .version
            .into_iter()
            .map(|v| Version::new(v.as_str()))
            .collect())
    }
}

#[cfg(test)]
pub(crate) mod integration_tests {
    use super::*;
    use env_logger;
    use log::error;
    use tokio;

    #[tokio::test]
    async fn test_maven_package_search() -> Result<()> {
        env_logger::init();

        let group_id = Group::new("dev.zio");
        let artifact_prefix = "zio_*";

        let maven_search = MavenPackageSearch::new();

        // Search for artifacts
        match maven_search
            .search_artifacts(&group_id, artifact_prefix)
            .await
        {
            Ok(response) => {
                println!("Repositories under group ID '{}':", group_id.value);
                for doc in response {
                    println!("Artifact ID: {}", doc.value);
                }
            }
            Err(e) => error!("Error searching for artifacts: {:?}", e),
        }

        let artifact_id = Artifact::new("zio_3");
        // Get versions for a specific artifact
        match maven_search.get_versions(&group_id, &artifact_id).await {
            Ok(versions) => println!("Versions for 'zio': {:?}", versions),
            Err(e) => error!("Error fetching versions: {:?}", e),
        }

        Ok(())
    }

    // get versions for https://repo1.maven.org/maven2/dev/zio/zio-json_2.13/
    #[tokio::test]
    async fn test_get_versions() -> Result<()> {
        let group_id = Group::new("dev.zio");
        let artifact_id = Artifact::new("zio-json_2.13");
        let maven_search = MavenPackageSearch::new();
        let versions = maven_search.get_versions(&group_id, &artifact_id).await?;
        println!("Versions for 'zio-json': {:?}", versions);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_github_repo() -> Result<()> {
        let group_id = Group::new("dev.zio");
        let artifact_id = Artifact::new("zio-json");
        let maven_search = MavenPackageSearch::new();
        let version = Version::new("0.7.0");

        let repo = maven_search
            .get_github_repo(&group_id, &artifact_id, &version)
            .await
            .unwrap();

        // https://github.com/zio/zio-json/releases/tag/v0.7.0
        let release_url = format!("{}/releases/tag/v{}", repo.unwrap(), version.to_string());

        // if webbrowser::open(&release_url).is_err() {
        //     println!("Failed to open the URL in the browser: {}", release_url);
        // }
        println!("{:?}", release_url);
        Ok(())
    }
}
