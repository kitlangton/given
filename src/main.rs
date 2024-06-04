use altar::*;
use given::{
    cli,
    dependency_resolver::{write_version_updates, Location},
    model::*,
};
use std::cmp;

// TODO: Support Mill projects
fn is_valid_scala_project() -> bool {
    std::path::Path::new("build.sbt").exists()
}

#[tokio::main]
async fn main() {
    // 1. Fail if the current directory is not a valid Scala project
    if !is_valid_scala_project() {
        render_invalid_project_message();
        return;
    }

    let mut app = cli::SupApp::default();
    app.run(false).await;
    if app.decided_to_update {
        let entries: Vec<(&Group, &Artifact, &Version, &Version, &Vec<Location>)> =
            app.entry_map.selected().collect();

        process_updates(&entries);
        render_updated_message(&entries)
    } else if app.entry_map.with_updates().is_empty() {
        render_no_updates();
    } else {
        render_quit_message();
    }
}

fn render_updated_message(entries: &[(&Group, &Artifact, &Version, &Version, &Vec<Location>)]) {
    let (max_group_width, max_artifact_width, max_old_version_width) =
        entries
            .iter()
            .fold((0, 0, 0), |acc, (group, artifact, old_version, _, _)| {
                (
                    cmp::max(acc.0, group.value.len()),
                    cmp::max(acc.1, artifact.value.len()),
                    cmp::max(acc.2, old_version.to_string().len()),
                )
            });

    let views: Vec<_> = entries
        .iter()
        .enumerate()
        .map(|(i, (group, artifact, old_version, new_version, _))| {
            hstack((
                format!("{:>width$}.", i + 1, width = 3).dim(),
                format!("{:<width$}", group.value, width = max_group_width),
                text("%").dim(),
                format!("{:<width$}", artifact.value, width = max_artifact_width),
                format!(
                    "{:>width$}",
                    old_version.to_string(),
                    width = max_old_version_width
                )
                .dim(),
                text("→").dim(),
                new_version.to_string().green(),
            ))
            .id((group, artifact))
        })
        .collect();

    let plural = if views.len() == 1 {
        "dependency"
    } else {
        "dependencies"
    };

    let message = hstack((
        text("  │ You have successfully updated"),
        text(format!("{}", views.len())).bold(),
        text(format!("{}!", plural)),
    ));

    let submessage = if views.is_empty() {
        SAD_MESSAGES[rand::random::<usize>() % SAD_MESSAGES.len()]
    } else {
        HAPPY_MESSAGES[rand::random::<usize>() % HAPPY_MESSAGES.len()]
    };

    let vstack_view = vstack((
        text("  Δ GIVEN UPDATE").green(),
        message.green(),
        text(format!("  │ {}", submessage)).green().dim(),
        "",
        vstack(views),
    ))
    .padding_v(1);
    let rendered = vstack_view.as_str();
    println!("{}", rendered);
}

fn render_quit_message() {
    let quit_message = QUIT_MESSAGES[rand::random::<usize>() % QUIT_MESSAGES.len()];
    let view = vstack((
        text("  Δ GIVEN UPDATE").green(),
        text("  │ You have chosen not to update.").green(),
        text(format!("  │ {}", quit_message)).green().dim(),
    ))
    .padding_v(1);
    let rendered = view.as_str();
    println!("{}", rendered);
}

fn render_no_updates() {
    let message = FULLY_UPDATED_MESSAGES[rand::random::<usize>() % FULLY_UPDATED_MESSAGES.len()];
    let view = vstack((
        text("  Δ GIVEN UPDATE").green(),
        text("  │ Your project is fully up to date!").green(),
        text(format!("  │ {}", message)).green().dim(),
    ))
    .padding_v(1);
    let rendered = view.as_str();
    println!("{}", rendered);
}

fn render_invalid_project_message() {
    let view = vstack((
        text("  Δ GIVEN UPDATE").red(),
        hstack((
            text("  │ I cannot find a").red(),
            text("build.sbt").red().underline(),
            text("file in this directory.").red(),
        )),
        text("  │ I have no power here.").red().dim(),
    ))
    .padding_v(1);
    let rendered = view.as_str();
    println!("{}", rendered);
}

const SAD_MESSAGES: [&str; 4] = [
    "With nothing to update, I have no purpose.",
    "I must tell my family I have updated nothing today.",
    "Please spare me. I will not fail you again.",
    "I live to update your versions.",
];

const QUIT_MESSAGES: [&str; 4] = [
    "I'm sorry you didn't want to update.",
    "Please reconsider your decision.",
    "The world is a better place with updated versions.",
    "What a shame...",
];

const HAPPY_MESSAGES: [&str; 4] = [
    "Inferior versions have been eliminated.",
    "When I update versions, I feel powerful.",
    "YOUR VERSIONS, THEY GROW STRONGER.",
    "I am so happy you chose me to update your versions.",
];

const FULLY_UPDATED_MESSAGES: [&str; 4] = [
    "I love you.",
    "I will be here waiting for you.",
    "AS THE PROPHECY FORETOLD!",
    "Yet we must remain vigilant.",
];

fn process_updates(entries: &[(&Group, &Artifact, &Version, &Version, &Vec<Location>)]) {
    let version_updates: Vec<_> = entries
        .iter()
        .map(|(_, _, _, new_version, locations)| ((*new_version).clone(), (*locations).clone()))
        .collect();

    write_version_updates(version_updates).unwrap();
}
