pub mod span;
pub use self::span::{Span, WithSpan};

use crate::model::{Artifact, Group, Version};

use std::collections::HashMap;
use tree_sitter::{Node, Query, QueryCursor, Tree};

use regex::Regex;

fn parse_dependency_from_captures(
    captures: &regex::Captures,
    val_defs: &HashMap<String, WithSpan<String>>,
) -> Option<(Group, Artifact, WithSpan<Version>)> {
    let group = Group::new(captures.get(1)?.as_str());
    let artifact = Artifact::new(captures.get(2)?.as_str());
    let version_or_identifier = captures.get(3)?.as_str();

    let version = if version_or_identifier.starts_with('"') && version_or_identifier.ends_with('"')
    {
        // Remove the double quotes and treat it as a version string
        let version_str = version_or_identifier.trim_matches('"');
        let position = Span::new(captures.get(3)?.start(), captures.get(3)?.end());
        WithSpan {
            value: Version::new(version_str),
            position,
        }
    } else if let Some(val) = val_defs.get(version_or_identifier) {
        WithSpan {
            value: Version::new(&val.value),
            position: val.position.clone(),
        }
    } else {
        // If it's an identifier and missing, return None
        return None;
    };

    Some((group, artifact, version))
}

fn extract_text(node: Node, code: &str) -> String {
    node.utf8_text(code.as_bytes())
        .unwrap()
        .to_string()
        .trim_matches('"')
        .to_string()
}

pub fn parse_dependencies(
    code: &str,
    val_defs: &HashMap<String, WithSpan<String>>,
) -> Vec<(Group, Artifact, WithSpan<Version>)> {
    // "(group)"" (%|%%|%%%) "(artifact)" % "(versionString)"|(identifier))

    // A quoted string representing the group, captured in the first group `([^"]+)`
    let group_pattern = r#""([^"]+)""#;

    // An optional separator which can be %, %%, or %%%, captured in the second group `(?:%|%%|%%%|)`
    let separator_pattern = r#"\s*(?:%|%%|%%%|)\s*"#; // Non-capturing group for separator

    // A quoted string representing the artifact, captured in the third group `([^"]+)`
    let artifact_pattern = r#""([^"]+)""#;

    // A % symbol followed by either a quoted string or an identifier representing the version, captured in the fourth group `([^"]+|[a-zA-Z_][a-zA-Z0-9_]*)`
    // let identifier_pattern = r#"[a-zA-Z_][a-zA-Z0-9_]*"#;
    let version_pattern = r#"\s*%\s*("([^"]+)"|[a-zA-Z_][a-zA-Z0-9_]*)"#; // Match either a quoted string or an identifier

    let full_pattern = format!(
        "{}{}{}{}",
        group_pattern, separator_pattern, artifact_pattern, version_pattern
    );
    let re = Regex::new(&full_pattern).unwrap();
    let mut dependencies = Vec::new();

    for captures in re.captures_iter(code) {
        if let Some(dependency) = parse_dependency_from_captures(&captures, val_defs) {
            dependencies.push(dependency);
        }
    }

    dependencies
}

fn parse_val(node: Node, code: &str) -> Option<(String, WithSpan<String>)> {
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
        WithSpan {
            value: rhs,
            position,
        },
    ))
}

fn extract_vals(node: Node, code: &str) -> HashMap<String, WithSpan<String>> {
    let mut vals = HashMap::new();
    parse_vals(node, code, &mut vals);
    vals
}

fn parse_vals(node: Node, code: &str, vals: &mut HashMap<String, WithSpan<String>>) {
    if node.kind() == "val_definition" {
        if let Some((name, value_with_position)) = parse_val(node, code) {
            vals.insert(name, value_with_position);
            return;
        }
    }
    for child in node.named_children(&mut node.walk()) {
        parse_vals(child, code, vals);
    }
}

fn parse_tree(code: &str) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_scala::language())
        .expect("Error loading Scala grammar");

    parser.parse(code, None).unwrap()
}

pub fn get_deps(code: &str) -> Vec<(Group, Artifact, WithSpan<Version>)> {
    let tree = parse_tree(code);
    let root_node = tree.root_node();
    let val_defs = extract_vals(root_node, code);
    parse_dependencies(code, &val_defs)
}

/// find the Scala version, as defined in the `scalaVersion := "..."` infix declaration
pub fn find_scala_version(
    code: &str,
    val_defs: &HashMap<String, WithSpan<String>>,
) -> Option<WithSpan<String>> {
    let scala_version_pattern = r#"scalaVersion\s*:=\s*("([^"]+)"|[a-zA-Z_][a-zA-Z0-9_]*)"#;
    let re = Regex::new(scala_version_pattern).unwrap();

    if let Some(captures) = re.captures(code) {
        let version_or_identifier = captures.get(1)?.as_str();
        let position = Span::new(captures.get(1)?.start(), captures.get(1)?.end());

        if version_or_identifier.starts_with('"') && version_or_identifier.ends_with('"') {
            // Remove the double quotes and treat it as a version string
            let version_str = version_or_identifier.trim_matches('"');
            return Some(WithSpan {
                value: version_str.to_string(),
                position,
            });
        } else if let Some(val) = val_defs.get(version_or_identifier) {
            return Some(WithSpan {
                value: val.value.clone(),
                position: val.position.clone(),
            });
        }
    }

    None
}

pub fn get_scala_version_from_build_sbt(
    code: &str,
) -> Option<(Group, Artifact, WithSpan<Version>)> {
    let tree = parse_tree(code);
    let root_node = tree.root_node();

    let vals = extract_vals(root_node, code);
    let scala_version = find_scala_version(code, &vals)?;
    let version = Version::new(&scala_version.value);
    let artifact_name = if version.major() == Some(3) {
        "scala3-library_3"
    } else {
        "scala-library"
    };
    Some((
        Group::new("org.scala-lang"),
        Artifact::new(artifact_name),
        WithSpan {
            value: version,
            position: scala_version.position,
        },
    ))
}

pub fn parse_dependencies_tree_sitter(code: &str) -> Vec<(Group, Artifact, WithSpan<Version>)> {
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
    let tree = parse_tree(code);
    let root_node = tree.root_node();
    let captures = query_cursor.captures(&query, root_node, code.as_bytes());

    let dependencies: Vec<_> = captures
        .filter_map(|(m, _)| {
            let group_node = m
                .captures
                .iter()
                .find(|c| query.capture_names()[c.index as usize] == "group")?
                .node;
            let artifact_node = m
                .captures
                .iter()
                .find(|c| query.capture_names()[c.index as usize] == "artifact")?
                .node;
            let version_node = m
                .captures
                .iter()
                .find(|c| query.capture_names()[c.index as usize] == "version")?
                .node;

            Some((
                Group::new(&extract_text(group_node, code)),
                Artifact::new(&extract_text(artifact_node, code)),
                WithSpan {
                    value: Version::new(&extract_text(version_node, code)),
                    position: Span::new(version_node.start_byte(), version_node.end_byte()),
                },
            ))
        })
        .collect();
    dependencies
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_scala_parser() {
        let code = r#"
        val animusVersion = "0.4.0"

        libraryDependencies ++= Seq(
          "dev.zio" %% "zio" % "2.0.0",
          "org.postgresql" % "postgresql" % "42.5.1",
          "io.github.kitlangton" % "animus" % animusVersion
        )

        val example = "example" %% "example" % "0.0.1"

        val falseExample = "hello" + "oops" + "0.0.1"

        libraryDependencies += "dev.zio" %% "zio-test" % "2.0.0" % Test
        "#;

        let dependencies = get_deps(code);
        println!("{:?}", dependencies);

        let expected_dependencies = vec![
            (
                Group::new("dev.zio"),
                Artifact::new("zio"),
                WithSpan {
                    value: Version::new("2.0.0"),
                    position: Span::new(106, 113),
                },
            ),
            (
                Group::new("org.postgresql"),
                Artifact::new("postgresql"),
                WithSpan {
                    value: Version::new("42.5.1"),
                    position: Span::new(159, 167),
                },
            ),
            (
                Group::new("io.github.kitlangton"),
                Artifact::new("animus"),
                WithSpan {
                    value: Version::new("0.4.0"),
                    position: Span::new(29, 36),
                },
            ),
            (
                Group::new("example"),
                Artifact::new("example"),
                WithSpan {
                    value: Version::new("0.0.1"),
                    position: Span::new(287, 294),
                },
            ),
            (
                Group::new("dev.zio"),
                Artifact::new("zio-test"),
                WithSpan {
                    value: Version::new("2.0.0"),
                    position: Span::new(408, 415),
                },
            ),
        ];
        assert_eq!(dependencies, expected_dependencies);
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

        let tree = parse_tree(code);
        let root_node = tree.root_node();

        let val_defs = extract_vals(root_node, code);
        let expected_val_defs = HashMap::from([
            (
                "example".to_string(),
                WithSpan {
                    value: "Hello".to_string(),
                    position: Span::new(58, 65),
                },
            ),
            (
                "falseExample".to_string(),
                WithSpan {
                    value: "123".to_string(),
                    position: Span::new(101, 104),
                },
            ),
            (
                "anotherExample".to_string(),
                WithSpan {
                    value: "World".to_string(),
                    position: Span::new(177, 184),
                },
            ),
            (
                "yetAnotherExample".to_string(),
                WithSpan {
                    value: "456".to_string(),
                    position: Span::new(229, 232),
                },
            ),
            (
                "complexExample".to_string(),
                WithSpan {
                    value: "Hello\" + \"World".to_string(),
                    position: Span::new(288, 305),
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

        let tree = parse_tree(code);
        let root_node = tree.root_node();

        let val_defs = extract_vals(root_node, code);
        let scala_version = find_scala_version(code, &val_defs);

        assert_eq!(
            scala_version,
            Some(WithSpan {
                value: "2.13.6".to_string(),
                position: Span::new(25, 33),
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

        let tree = parse_tree(code);
        let root_node = tree.root_node();

        let val_defs = extract_vals(root_node, code);
        let scala_version = find_scala_version(code, &val_defs);

        assert_eq!(
            scala_version,
            Some(WithSpan {
                value: "2.12.8".to_string(),
                position: Span::new(52, 60),
            })
        );
    }

    #[test]
    fn test_scala_version_rhs_extraction_to_variable() {
        let code = r#"
        val scala3 = "3.4.2"
        scalaVersion := scala3
    "#;

        let scala_version = get_scala_version_from_build_sbt(code);

        assert_eq!(
            scala_version,
            Some((
                Group::new("org.scala-lang"),
                Artifact::new("scala3-library_3"),
                WithSpan {
                    value: Version::new("3.4.2"),
                    position: Span::new(22, 29),
                }
            ))
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

        let dependencies = parse_dependencies_tree_sitter(code);
        for (group, artifact, version) in dependencies {
            println!(
                "Group: {}, Artifact: {}, Version: {}",
                group.value, artifact.value, version.value
            );
        }
    }
}
