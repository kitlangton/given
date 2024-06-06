pub mod span;
pub use self::span::{Span, WithSpan};

use crate::{
    dependency_resolver::Location,
    model::{Artifact, Group, Version},
};

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tree_sitter::{Node, Query, QueryCursor, Tree};

use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct WithLocation<T> {
    pub value: T,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub group: Group,
    pub artifact: Artifact,
    pub version: WithLocation<Version>,
}

pub struct DependencyParser {
    pub val_defs: HashMap<String, WithLocation<String>>,
    pub dependencies: Vec<Dependency>,
}

impl Default for DependencyParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyParser {
    pub fn new() -> Self {
        Self {
            val_defs: HashMap::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn parse_val_defs(&mut self, source: &Path, code: &str) {
        let tree = parse_tree(code);
        let root_node = tree.root_node();
        parse_vals(source, root_node, code, &mut self.val_defs);
    }

    pub fn parse_dependencies(&mut self, source: &Path, code: &str) {
        let tree = parse_tree(code);
        let root_node = tree.root_node();
        let dependencies = parse_dependencies(source, code, &root_node, &self.val_defs);
        self.dependencies.extend(dependencies);
    }
}

fn extract_text(node: Node, code: &str) -> String {
    node.utf8_text(code.as_bytes())
        .unwrap()
        .to_string()
        .trim_matches('"')
        .to_string()
}

// TODO: Use tree-sitter Query
fn parse_val(source: &Path, node: Node, code: &str) -> Option<(String, WithLocation<String>)> {
    let mut cursor = node.walk();
    let mut children = node.named_children(&mut cursor);

    // drop until we get to the identifier, this handles "lazy" vals (dropping the modifier)
    let ident_node = children.find(|node| node.kind() == "identifier")?;
    let rhs_node = children.next()?;

    let ident = extract_text(ident_node, code);
    let rhs = extract_text(rhs_node, code);
    let position = Span::new(rhs_node.start_byte(), rhs_node.end_byte());

    Some((
        ident,
        WithLocation {
            value: rhs,
            location: Location::new(PathBuf::from(source), position),
        },
    ))
}

fn extract_vals(source: &Path, node: Node, code: &str) -> HashMap<String, WithLocation<String>> {
    let mut vals = HashMap::new();
    parse_vals(source, node, code, &mut vals);
    vals
}

fn parse_vals(
    source: &Path,
    node: Node,
    code: &str,
    vals: &mut HashMap<String, WithLocation<String>>,
) {
    if node.kind() == "val_definition" {
        if let Some((name, value_with_position)) = parse_val(source, node, code) {
            vals.insert(name, value_with_position);
            return;
        }
    }
    for child in node.named_children(&mut node.walk()) {
        parse_vals(source, child, code, vals);
    }
}

fn parse_tree(code: &str) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_scala::language())
        .expect("Error loading Scala grammar");

    parser.parse(code, None).unwrap()
}

/// find the Scala version, as defined in the `scalaVersion := "..."` infix declaration
/// TODO: Use tree-sitter
pub fn find_scala_version(
    source: &Path,
    code: &str,
    val_defs: &HashMap<String, WithLocation<String>>,
) -> Option<WithLocation<Version>> {
    let scala_version_pattern = r#"scalaVersion\s*:=\s*("([^"]+)"|[a-zA-Z_][a-zA-Z0-9_]*)"#;
    let re = Regex::new(scala_version_pattern).unwrap();

    if let Some(captures) = re.captures(code) {
        let version_or_identifier = captures.get(1)?.as_str();
        let position = Span::new(captures.get(1)?.start(), captures.get(1)?.end());

        if version_or_identifier.starts_with('"') && version_or_identifier.ends_with('"') {
            let version_str = version_or_identifier.trim_matches('"');
            return Some(WithLocation {
                value: Version::new(version_str),
                location: Location::new(PathBuf::from(source), position),
            });
        } else if let Some(val) = val_defs.get(version_or_identifier) {
            return Some(WithLocation {
                value: Version::new(&val.value),
                location: val.location.clone(),
            });
        }
    }

    None
}

pub fn get_scala_version_from_build_sbt(source: &Path, code: &str) -> Option<Dependency> {
    let tree = parse_tree(code);
    let root_node = tree.root_node();

    let vals = extract_vals(source, root_node, code);
    let scala_version = find_scala_version(source, code, &vals)?;
    let artifact_name = if scala_version.value.major() == Some(3) {
        "scala3-library_3"
    } else {
        "scala-library"
    };
    Some(Dependency {
        group: Group::new("org.scala-lang"),
        artifact: Artifact::new(artifact_name),
        version: scala_version,
    })
}

pub fn parse_dependencies(
    source: &Path,
    code: &str,
    node: &Node,
    val_defs: &HashMap<String, WithLocation<String>>,
) -> Vec<Dependency> {
    let query = r#"
    [
    (infix_expression
        left: (infix_expression
            left: (string) @group
            operator: (operator_identifier) @percents
            right: (string) @artifact
        )
        operator: (operator_identifier) @percent
        right: (_) @version
    )
    (infix_expression
        left: (infix_expression
            left: (infix_expression 
                left: (identifier) 
                operator: (operator_identifier) 
                right: (string) @group
            )
            operator: (_) @percents
            right: (_) @artifact
        )
        operator: (_) @percent
        right: (_) @version
    )
    ]
    "#;

    let mut query_cursor = QueryCursor::new();
    let query = Query::new(&tree_sitter_scala::language(), query).unwrap();
    let captures = query_cursor.captures(&query, *node, code.as_bytes());

    let dependencies: Vec<_> = captures
        .filter_map(|(m, _)| {
            let mut group_node = None;
            let mut percents_node = None;
            let mut artifact_node = None;
            let mut percent_node = None;
            let mut version_node = None;

            for capture in m.captures.iter() {
                match query.capture_names()[capture.index as usize] {
                    "percents" => percents_node = Some(capture.node),
                    "percent" => percent_node = Some(capture.node),
                    "group" => group_node = Some(capture.node),
                    "artifact" => artifact_node = Some(capture.node),
                    "version" => version_node = Some(capture.node),
                    _ => {}
                }
            }

            let percents_text = extract_text(percents_node?, code);
            if !percents_text.chars().all(|c| c == '%') {
                return None;
            }

            let percent_text = extract_text(percent_node?, code);
            if !percent_text.chars().all(|c| c == '%') {
                return None;
            }

            let version_node = version_node?;
            let version = match version_node.kind() {
                "string" => WithLocation {
                    value: Version::new(&extract_text(version_node, code)),
                    location: Location::new(
                        PathBuf::from(source),
                        Span::new(version_node.start_byte(), version_node.end_byte()),
                    ),
                },
                "identifier" => {
                    let ident = extract_text(version_node, code);
                    let val = val_defs.get(&ident)?;
                    WithLocation {
                        value: Version::new(&val.value),
                        location: val.location.clone(),
                    }
                }
                _ => {
                    let ident = parse_select(version_node, code)?;
                    let val = val_defs.get(&ident)?;
                    WithLocation {
                        value: Version::new(&val.value),
                        location: val.location.clone(),
                    }
                }
            };

            Some(Dependency {
                group: Group::new(&extract_text(group_node?, code)),
                artifact: Artifact::new(&extract_text(artifact_node?, code)),
                version,
            })
        })
        .collect();
    dependencies
}

// Versions.version -> version
// Thing.Other.version -> version
// version -> version
fn parse_select(node: Node, code: &str) -> Option<String> {
    let mut cursor = node.walk();
    let mut children = node.named_children(&mut cursor);

    // Check if the node is an identifier
    if node.kind() == "identifier" {
        return Some(extract_text(node, code));
    }

    // Iterate through the children to find the last identifier
    let mut last_identifier = None;
    while let Some(child) = children.next() {
        if child.kind() == "identifier" {
            last_identifier = Some(child);
        }
    }

    // Extract the text of the last identifier if it exists
    last_identifier.map(|ident_node| extract_text(ident_node, code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_scala_parser() {
        let code = r#"
        val animusVersion = "0.4.0"
        object Versions {
            val neotype = "0.1.0"
        }

        libraryDependencies ++= Seq(
          "io.github.kitlangton" %% "neotype" % Versions.neotype,
          "dev.zio" %% "zio" % "2.0.0",
          "org.postgresql" % "postgresql" % "42.5.1",
          "io.github.kitlangton" % "animus" % animusVersion
        )

        val example = "example" %% "example" % "0.0.1"

        val falseExample = "hello" + "oops" + "0.0.1"

        libraryDependencies += "dev.zio" %% "zio-test" % "2.0.0" % Test
        "#;

        let source = PathBuf::from("example.scala");
        let mut parser = DependencyParser::new();
        parser.parse_val_defs(&source, code);
        parser.parse_dependencies(&source, code);
        println!("{:?}", parser.dependencies);

        let expected_dependencies = vec![
            Dependency {
                group: Group::new("io.github.kitlangton"),
                artifact: Artifact::new("neotype"),
                version: WithLocation {
                    value: Version::new("0.1.0"),
                    location: Location::new(source.clone(), Span::new(89, 96)),
                },
            },
            Dependency {
                group: Group::new("dev.zio"),
                artifact: Artifact::new("zio"),
                version: WithLocation {
                    value: Version::new("2.0.0"),
                    location: Location::new(source.clone(), Span::new(242, 249)),
                },
            },
            Dependency {
                group: Group::new("org.postgresql"),
                artifact: Artifact::new("postgresql"),
                version: WithLocation {
                    value: Version::new("42.5.1"),
                    location: Location::new(source.clone(), Span::new(295, 303)),
                },
            },
            Dependency {
                group: Group::new("io.github.kitlangton"),
                artifact: Artifact::new("animus"),
                version: WithLocation {
                    value: Version::new("0.4.0"),
                    location: Location::new(source.clone(), Span::new(29, 36)),
                },
            },
            Dependency {
                group: Group::new("example"),
                artifact: Artifact::new("example"),
                version: WithLocation {
                    value: Version::new("0.0.1"),
                    location: Location::new(source.clone(), Span::new(423, 430)),
                },
            },
            Dependency {
                group: Group::new("dev.zio"),
                artifact: Artifact::new("zio-test"),
                version: WithLocation {
                    value: Version::new("2.0.0"),
                    location: Location::new(source.clone(), Span::new(544, 551)),
                },
            },
        ];
        assert_eq!(parser.dependencies, expected_dependencies);
    }

    #[test]
    fn test_extract_vals() {
        let code = r#"
            object Outer {
                val example = "Hello"
                val falseExample = 123
                object Inner {
                    val anotherExample = "World"
                    val yetAnotherExample = 456
                }
                val complexExample = "Hello" + "World"
            }
            "#;

        let source = PathBuf::from("example.scala");
        let tree = parse_tree(code);
        let root_node = tree.root_node();

        let val_defs = extract_vals(&source, root_node, code);
        let expected_val_defs = HashMap::from([
            (
                "example".to_string(),
                WithLocation {
                    value: "Hello".to_string(),
                    location: Location::new(source.clone(), Span::new(58, 65)),
                },
            ),
            (
                "falseExample".to_string(),
                WithLocation {
                    value: "123".to_string(),
                    location: Location::new(source.clone(), Span::new(101, 104)),
                },
            ),
            (
                "anotherExample".to_string(),
                WithLocation {
                    value: "World".to_string(),
                    location: Location::new(source.clone(), Span::new(177, 184)),
                },
            ),
            (
                "yetAnotherExample".to_string(),
                WithLocation {
                    value: "456".to_string(),
                    location: Location::new(source.clone(), Span::new(229, 232)),
                },
            ),
            (
                "complexExample".to_string(),
                WithLocation {
                    value: "Hello\" + \"World".to_string(),
                    location: Location::new(source.clone(), Span::new(288, 305)),
                },
            ),
        ]);
        assert_eq!(val_defs, expected_val_defs);
    }

    #[test]
    fn test_scala_version_extraction() {
        let code = r#"
        scalaVersion := "2.13.6"
    "#;

        let source = PathBuf::from("example.scala");
        let tree = parse_tree(code);
        let root_node = tree.root_node();

        let val_defs = extract_vals(&source, root_node, code);
        let scala_version = find_scala_version(&source, code, &val_defs);

        assert_eq!(
            scala_version,
            Some(WithLocation {
                value: Version::new("2.13.6"),
                location: Location::new(source.clone(), Span::new(25, 33)),
            })
        );
    }

    #[test]
    fn test_scala_version_in_common_settings() {
        let code = r#"
    commonSettings := Seq(
        scalaVersion := "2.12.8",
        organization := "com.example"
    )
    "#;

        let source = PathBuf::from("example.scala");
        let tree = parse_tree(code);
        let root_node = tree.root_node();

        let val_defs = extract_vals(&source, root_node, code);
        let scala_version = find_scala_version(&source, code, &val_defs);

        assert_eq!(
            scala_version,
            Some(WithLocation {
                value: Version::new("2.12.8"),
                location: Location::new(source.clone(), Span::new(52, 60)),
            })
        );
    }

    #[test]
    fn test_scala_version_rhs_extraction_to_variable() {
        let code = r#"
        val scala3 = "3.4.2"
        scalaVersion := scala3
    "#;

        let source = PathBuf::from("example.scala");
        let scala_version = get_scala_version_from_build_sbt(&source, code);

        assert_eq!(
            scala_version,
            Some(Dependency {
                group: Group::new("org.scala-lang"),
                artifact: Artifact::new("scala3-library_3"),
                version: WithLocation {
                    value: Version::new("3.4.2"),
                    location: Location::new(source.clone(), Span::new(22, 29)),
                }
            })
        );
    }

    #[test]
    fn test_lazy_vals() {
        let code = r#"
        lazy val scala2 = "2.13.6"
        scalaVersion := scala2
    "#;

        // INSERT_YOUR_CODE
        let query = r#"
        (val_definition
            pattern: (identifier) @name
            value: (expression) @value
        )
        "#;

        let mut query_cursor = QueryCursor::new();
        let query = Query::new(&tree_sitter_scala::language(), query).unwrap();
        let tree = parse_tree(code);
        let root_node = tree.root_node();
        let captures = query_cursor.captures(&query, root_node, code.as_bytes());

        for (m, _) in captures {
            let name_capture = m
                .captures
                .iter()
                .find(|c| query.capture_names()[c.index as usize] == "name")
                .unwrap();
            let value_capture = m
                .captures
                .iter()
                .find(|c| query.capture_names()[c.index as usize] == "value")
                .unwrap();

            let name_node = name_capture.node;
            let value_node = value_capture.node;

            let name = extract_text(name_node, code);
            let value = extract_text(value_node, code);
            let position = Span::new(value_node.start_byte(), value_node.end_byte());

            println!("Name: {}", name);
            println!("Value: {}", value);
            println!("Position: {:?}", position);
        }
    }

    #[test]
    fn test_extract_dependencies() {
        let code = r#"
        libraryDependencies += "dev.zio" %% "zio" % "1.0.0"
        libraryDependencies ++= Seq(
            "dev.zio" %% "zio-json" % "1.5.0",
            "dev.zio" %% "zio-test" % "2.0.0" % Test
        )
        libraryDependencies += "com.typesafe.akka" %% "akka-actor" % "2.6.14"
        libraryDependencies ++= Seq(
            "org.scalatest" %% "scalatest" % "3.2.9" % Test,
            "com.lihaoyi" %%% "upickle" % "1.4.0"A % "provided",
        )
        libraryDependencies += "org.typelevel" %% "cats-core" % "2.6.1"
        libraryDependencies ++= Seq(
            "org.http4s" %% "http4s-dsl" % "0.21.23",
            "org.http4s" % "http4s-blaze-server" % "0.21.23",
            "org.http4s" %% "http4s-circe" % "0.21.23"
        )
        "#;

        let source = PathBuf::from("example.scala");
        let tree = parse_tree(code);
        let node = tree.root_node();
        let val_defs = extract_vals(&source, node, code);
        let dependencies = parse_dependencies(&source, code, &node, &val_defs);
        for dependency in dependencies {
            println!(
                "Group: {}, Artifact: {}, Version: {}",
                dependency.group.value, dependency.artifact.value, dependency.version.value
            );
        }
    }
}
