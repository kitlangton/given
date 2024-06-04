use std::collections::HashMap;

use itertools::Itertools;

use crate::dependency_resolver::{DependencyMap, Location};
use crate::model::{
    update_options::{UpdateOptions, VersionType},
    Artifact, Group, Version,
};

#[derive(Clone, Debug)]
pub struct Entry {
    pub group: Group,
    pub artifact: Artifact,
    pub version: Version,
    pub locations: Vec<Location>,
    pub update_options: Option<UpdateOptions>,
    pub version_type: VersionType,
    pub is_selected: bool,
}

impl Entry {
    pub fn current_update_version(&self) -> Option<&Version> {
        if let Some(update_options) = &self.update_options {
            match self.version_type {
                VersionType::Major => update_options.major.as_ref(),
                VersionType::Minor => update_options.minor.as_ref(),
                VersionType::Patch => update_options.patch.as_ref(),
                VersionType::PreRelease => update_options.pre_release.as_ref(),
            }
        } else {
            None
        }
    }
}

pub struct EntryMap {
    pub map: HashMap<(Group, Artifact), Entry>,
}

impl Default for EntryMap {
    fn default() -> Self {
        Self::new()
    }
}

impl EntryMap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn with_updates(&self) -> Vec<(Group, Artifact, Entry)> {
        self.map
            .iter()
            .filter_map(|(dep, entry)| {
                entry
                    .update_options
                    .as_ref()
                    .map(|_| (dep.0.clone(), dep.1.clone(), entry.clone()))
            })
            .sorted_by(|(group, artifact, _), (b_group, b_artifact, _)| {
                (group, artifact).cmp(&(b_group, b_artifact))
            })
            .collect()
    }

    pub fn get(&self, group: &Group, artifact: &Artifact) -> Option<&Entry> {
        self.map.get(&(group.clone(), artifact.clone()))
    }

    pub fn get_mut(&mut self, group: &Group, artifact: &Artifact) -> Option<&mut Entry> {
        self.map.get_mut(&(group.clone(), artifact.clone()))
    }

    pub fn insert(&mut self, group: Group, artifact: Artifact, entry: Entry) {
        self.map.insert((group, artifact), entry);
    }

    pub fn remove(&mut self, group: &Group, artifact: &Artifact) -> Option<Entry> {
        self.map.remove(&(group.clone(), artifact.clone()))
    }

    pub fn groups_and_artifacts(&self) -> Vec<(Group, Artifact)> {
        self.map.keys().cloned().collect()
    }

    fn version_type_exists(update_options: &UpdateOptions, version_type: VersionType) -> bool {
        match version_type {
            VersionType::Major => update_options.major.is_some(),
            VersionType::Minor => update_options.minor.is_some(),
            VersionType::Patch => update_options.patch.is_some(),
            VersionType::PreRelease => update_options.pre_release.is_some(),
        }
    }

    pub fn add_versions(&mut self, versions_map: &HashMap<(Group, Artifact), Vec<Version>>) {
        versions_map
            .iter()
            .for_each(|((group, artifact), versions)| {
                if let Some(entry) = self.get_mut(group, artifact) {
                    if let Some(update_options) = UpdateOptions::new(&entry.version, versions) {
                        // println!(
                        //     "Update options for {:?}: {:?} current: {} all:  {}",
                        //     (group, artifact),
                        //     update_options,
                        //     entry.version,
                        //     versions.iter().join(", ")
                        // );
                        let version_type = Self::determine_version_type(&update_options);
                        entry.update_options = Some(update_options);
                        entry.version_type = version_type;
                    }
                } else {
                    panic!(
                        "Entry for group {:?} and artifact {:?} does not exist",
                        group, artifact
                    );
                }
            });
    }

    fn determine_version_type(update_options: &UpdateOptions) -> VersionType {
        if update_options.major.is_some() {
            VersionType::Major
        } else if update_options.minor.is_some() {
            VersionType::Minor
        } else if update_options.patch.is_some() {
            VersionType::Patch
        } else {
            VersionType::PreRelease
        }
    }

    pub fn from_dependency_map(dependencies: &DependencyMap) -> EntryMap {
        let mut entry_map = EntryMap::new();
        for ((group, artifact), version_with_locations) in dependencies.iter() {
            let version = version_with_locations.version.clone();
            entry_map.insert(
                group.clone(),
                artifact.clone(),
                Entry {
                    group: group.clone(),
                    artifact: artifact.clone(),
                    version,
                    locations: version_with_locations.locations.clone(),
                    update_options: None,
                    version_type: VersionType::Major,
                    is_selected: false,
                },
            );
        }
        entry_map
    }

    /// should return an iter of the Group, Artifact, Version, and Locations for each selected item.
    pub fn selected(
        &self,
    ) -> impl Iterator<Item = (&Group, &Artifact, &Version, &Version, &Vec<Location>)> {
        self.map.iter().filter_map(|((group, artifact), entry)| {
            if entry.is_selected {
                entry.update_options.as_ref().and_then(|update_options| {
                    let version = match entry.version_type {
                        VersionType::Major => update_options.major.as_ref(),
                        VersionType::Minor => update_options.minor.as_ref(),
                        VersionType::Patch => update_options.patch.as_ref(),
                        VersionType::PreRelease => update_options.pre_release.as_ref(),
                    };
                    version.map(|v| (group, artifact, &entry.version, v, &entry.locations))
                })
            } else {
                None
            }
        })
    }

    fn for_each_shared_entry<F>(&mut self, group: &Group, artifact: &Artifact, mut f: F)
    where
        F: FnMut(&mut Entry),
    {
        if let Some(entry) = self.get(group, artifact) {
            let locations = entry.locations.clone();
            for ((_, _), e) in self.map.iter_mut() {
                if e.locations.iter().any(|loc| locations.contains(loc)) {
                    f(e);
                }
            }
        }
    }

    pub fn toggle_selection(&mut self, group: &Group, artifact: &Artifact) {
        self.for_each_shared_entry(group, artifact, |e| {
            e.is_selected = !e.is_selected;
        });
    }

    pub(crate) fn deselect(&mut self, group: &Group, artifact: &Artifact) {
        if let Some(entry) = self.get_mut(group, artifact) {
            entry.is_selected = false;
        }
    }

    pub(crate) fn select(&mut self, group: &Group, artifact: &Artifact) {
        if let Some(entry) = self.get_mut(group, artifact) {
            entry.is_selected = true;
        }
    }

    pub fn next_version_type(&mut self, group: &Group, artifact: &Artifact) {
        self.change_version_type(group, artifact, |vt| vt.next());
    }

    pub fn prev_version_type(&mut self, group: &Group, artifact: &Artifact) {
        self.change_version_type(group, artifact, |vt| vt.prev());
    }

    fn change_version_type<F>(&mut self, group: &Group, artifact: &Artifact, change_fn: F)
    where
        F: Fn(VersionType) -> VersionType,
    {
        self.for_each_shared_entry(group, artifact, |e| {
            if let Some(update_options) = &e.update_options {
                loop {
                    e.version_type = change_fn(e.version_type);
                    if Self::version_type_exists(update_options, e.version_type) {
                        break;
                    }
                }
            }
        });
    }
}
