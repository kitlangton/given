mod entry_map;

use altar::*;
pub use entry_map::EntryMap;

use std::{collections::HashMap, sync::Arc};

use crate::{
    dependency_resolver::{self},
    model::{
        update_options::{UpdateOptions, VersionType},
        Artifact, Group, Version,
    },
    package_search::{maven::MavenPackageSearch, PackageSearchExt},
};

pub struct SupApp {
    pub entry_map: EntryMap,
    maven_package_search: Arc<MavenPackageSearch>,
    selected_index: u16,
    show_group: bool,
    pub decided_to_update: bool,
}

impl Default for SupApp {
    fn default() -> Self {
        Self {
            entry_map: EntryMap::new(),
            maven_package_search: Arc::new(MavenPackageSearch::new()),
            selected_index: 0,
            show_group: false,
            decided_to_update: false,
        }
    }
}

impl SupApp {
    fn toggle_show_group(&mut self) {
        self.show_group = !self.show_group;
    }

    fn change_version(&mut self, direction: i8) {
        if let Some((group, artifact, _)) = self
            .entry_map
            .with_updates()
            .get(self.selected_index as usize)
        {
            if direction > 0 {
                self.entry_map.next_version_type(group, artifact);
            } else {
                self.entry_map.prev_version_type(group, artifact);
            }
        }
    }

    fn next_version(&mut self) {
        self.change_version(1);
    }

    fn prev_version(&mut self) {
        self.change_version(-1);
    }

    fn toggle_selection(&mut self) {
        if let Some((group, artifact, _)) = self
            .entry_map
            .with_updates()
            .get(self.selected_index as usize)
        {
            self.entry_map.toggle_selection(group, artifact);
        }
    }

    fn toggle_all_selections(&mut self) {
        let all_selected = self
            .entry_map
            .with_updates()
            .iter()
            .all(|(_, _, entry)| entry.is_selected);

        for (group, artifact, _) in self.entry_map.with_updates() {
            if all_selected {
                self.entry_map.deselect(&group, &artifact);
            } else {
                self.entry_map.select(&group, &artifact);
            }
        }
    }
}

fn render_update_options(
    is_selected: bool,
    is_current: bool,
    update_options: &UpdateOptions,
    version_type: &VersionType,
) -> impl View {
    let render_version_option = |option: Option<Version>, target_type: VersionType| {
        let is_selected_type = *version_type == target_type;
        option
            .map(|v| text(format!("{}", v)))
            .underline_when(is_selected && is_selected_type)
            .dim_when(!is_selected_type)
            .bold_when(is_current && is_selected_type)
    };

    hstack((
        render_version_option(update_options.major.clone(), VersionType::Major),
        render_version_option(update_options.minor.clone(), VersionType::Minor),
        render_version_option(update_options.patch.clone(), VersionType::Patch),
        render_version_option(update_options.pre_release.clone(), VersionType::PreRelease)
            .magenta(),
        text(format!("{}", version_type))
            .yellow()
            .visible(is_current),
    ))
    .green()
}

fn render_dependency(
    show_group: bool,
    is_current: bool,
    entry: &entry_map::Entry,
    group_width: usize,
    artifact_width: usize,
    version_width: usize,
) -> impl View {
    let circle = if entry.is_selected { "●" } else { "○" };
    let circle_color = if entry.is_selected {
        Color::DarkGreen
    } else {
        Color::Reset
    };

    hstack((
        text(if is_current { "❯" } else { " " }).green(),
        text(circle)
            .color(circle_color)
            .dim_when(!is_current && !entry.is_selected),
        text(format!(
            "{:<width$}",
            entry.group.value,
            width = group_width
        ))
        .bold_when(is_current)
        .visible(show_group),
        text("%").dim().visible(show_group),
        text(format!(
            "{:<width$}",
            entry.artifact.value,
            width = artifact_width
        ))
        .bold_when(is_current),
        text(format!(
            "{:>width$}",
            entry.version.to_string(),
            width = version_width
        ))
        .dim(),
        text("→").dim(),
        render_update_options(
            entry.is_selected,
            is_current,
            entry.update_options.as_ref().unwrap(),
            &entry.version_type,
        ),
    ))
}

fn render_dependencies(
    dependencies: &[(Group, Artifact, entry_map::Entry)],
    selected_index: u16,
    show_group: bool,
) -> impl View {
    let (group_width, artifact_width, version_width) =
        dependencies
            .iter()
            .fold((0, 0, 0), |(gw, aw, vw), (group, artifact, entry)| {
                (
                    gw.max(group.value.len()),
                    aw.max(artifact.value.len()),
                    vw.max(entry.version.to_string().len()),
                )
            });

    vstack(
        dependencies
            .iter()
            .enumerate()
            .map(|(index, (group, artifact, entry))| {
                render_dependency(
                    show_group,
                    selected_index == index as u16,
                    entry,
                    group_width,
                    artifact_width,
                    version_width,
                )
                .id((group, artifact))
            })
            .collect::<Vec<_>>(),
    )
}

fn render_command(key: &str, label: &str) -> impl View {
    hstack((text(key), text(label).dim()))
}

fn render_commands(show_group: bool) -> impl View {
    let show_groups_text = if show_group {
        "hide groups"
    } else {
        "show groups"
    };
    hstack((
        render_command("space", "toggle"),
        render_command("a", "toggle all"),
        render_command("g", show_groups_text),
        render_command("q", "quit"),
    ))
    .spacing(2)
    .cyan()
    .padding_h(2)
}

#[derive(Debug)]
pub enum Message {
    VersionsRetrieved(HashMap<(Group, Artifact), Vec<Version>>),
}

impl AsyncTerminalApp for SupApp {
    type Message = Message;

    fn handle_exit(&self) -> Option<impl View> {
        Some("")
    }

    fn render(&self) -> impl View {
        let dependencies = self.entry_map.with_updates();
        let selected_index = self.selected_index;
        let show_group = self.show_group;

        if dependencies.is_empty() {
            hstack((
                text("  Δ GIVEN UPDATE").green(),
                text("LOADING...").green().dim(),
            ))
            .padding_v(1)
            .as_any()
            .id("Loading")
        } else {
            vstack((
                text("  Δ GIVEN UPDATE").green(),
                "",
                render_dependencies(&dependencies, selected_index, show_group),
                "",
                render_commands(self.show_group),
            ))
            .padding_v(1)
            .as_any()
            .id("Loaded")
        }
    }

    fn update(
        &mut self,
        event: Event<Self::Message>,
        _sender: &tokio::sync::mpsc::UnboundedSender<Self::Message>,
    ) -> bool {
        match event {
            Event::Key(key) => match key.code {
                KeyCode::Char('q') => return false,
                KeyCode::Char('j') | KeyCode::Down => {
                    self.selected_index =
                        (self.selected_index + 1) % self.entry_map.with_updates().len() as u16;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected_index = if self.selected_index == 0 {
                        self.entry_map.with_updates().len() as u16 - 1
                    } else {
                        self.selected_index - 1
                    };
                }
                KeyCode::Right | KeyCode::Tab => {
                    self.next_version();
                }
                KeyCode::Left | KeyCode::BackTab => {
                    self.prev_version();
                }
                KeyCode::Char(' ') => {
                    self.toggle_selection();
                }
                KeyCode::Char('a') => {
                    self.toggle_all_selections();
                }
                KeyCode::Char('g') => {
                    self.toggle_show_group();
                }
                KeyCode::Char('o') => {
                    let (group, artifact, entry) = {
                        let entry_map = self.entry_map.with_updates();
                        entry_map.get(self.selected_index as usize).unwrap().clone()
                    };

                    if let Some(version) = entry.current_update_version() {
                        let group = group.clone();
                        let artifact = artifact.clone();
                        let version = version.clone();
                        tokio::spawn(async move {
                            let search = MavenPackageSearch::new();
                            if let Ok(Some(github_url)) =
                                search.get_github_repo(&group, &artifact, &version).await
                            {
                                let release_url =
                                    format!("{}/releases/tag/v{}", github_url, version);
                                let _ = webbrowser::open(&release_url);
                            }
                        });
                    }
                }
                KeyCode::Enter => {
                    self.decided_to_update = true;
                    return false;
                }
                _ => (),
            },
            Event::Message(Message::VersionsRetrieved(versions_map)) => {
                self.entry_map.add_versions(&versions_map);

                if self.entry_map.with_updates().is_empty() {
                    return false;
                }
            }
        }
        true
    }

    fn init(&mut self, sender: &tokio::sync::mpsc::UnboundedSender<Self::Message>) {
        let current_dir = std::env::current_dir().unwrap();
        let dependencies = dependency_resolver::collect_sbt_dependencies(&current_dir).unwrap();
        self.entry_map = EntryMap::from_dependency_map(&dependencies);

        let all_groups_and_artifacts = self.entry_map.groups_and_artifacts();
        let maven_package_search = self.maven_package_search.clone();
        let sender_clone = sender.clone();

        let maybe_scala_version = dependencies
            .iter()
            .find(|((group, artifact), _)| {
                group.value == "org.scala-lang"
                    && (artifact.value == "scala-library" || artifact.value == "scala3-library_3")
            })
            .map(|((_, _), version)| (version.version.clone()));

        tokio::spawn(async move {
            let versions_map = maven_package_search
                .get_multiple_versions(all_groups_and_artifacts, maybe_scala_version)
                .await
                .unwrap_or_default();

            let _ = sender_clone.send(Message::VersionsRetrieved(versions_map));
        });
    }
}
