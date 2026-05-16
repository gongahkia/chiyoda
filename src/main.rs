use gibson_rust::bundle::create_bundle;
use gibson_rust::cli::{usage, RuntimeOptions};
use gibson_rust::generation::generate_saved_structure_with_rules;
use gibson_rust::inspect::{render_inspection, render_inspection_json};
use gibson_rust::rules::validate_rule_file;
use gibson_rust::scenario::{generate_scenario, save_scenario};
use gibson_rust::structure::{self, CURRENT_SEED_FILE};
use gibson_rust::validation::validate_file;

fn main() {
    let options = match RuntimeOptions::from_env() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };

    if options.validate_rules_path.is_some() {
        if let Err(error) = validate_rules(&options) {
            eprintln!("Failed to validate rules: {error}");
            std::process::exit(1);
        }
        return;
    } else if options.bundle_path.is_some() {
        if let Err(error) = export_bundle(&options) {
            eprintln!("Failed to export bundle: {error}");
            std::process::exit(1);
        }
        return;
    } else if options.validate_path.is_some() {
        if let Err(error) = validate_artifact(&options) {
            eprintln!("Failed to validate artifact: {error}");
            std::process::exit(1);
        }
        return;
    } else if options.inspect_path.is_some() {
        if let Err(error) = inspect_saved_structure(&options) {
            eprintln!("Failed to inspect structure: {error}");
            std::process::exit(1);
        }
        return;
    } else if options.headless {
        if let Err(error) = export_headless(&options) {
            eprintln!("Failed to export generated structure: {error}");
            std::process::exit(1);
        }
        return;
    }

    macroquad::Window::from_config(
        gibson_rust::render::window_conf(),
        gibson_rust::render::run(options),
    );
}

fn export_bundle(options: &RuntimeOptions) -> structure::StructureResult<()> {
    let path = options
        .bundle_path
        .as_ref()
        .expect("bundle path checked before dispatch");
    create_bundle(
        path,
        options.seed.clone(),
        options.config.clone(),
        options.rule_packs.clone(),
    )
}

fn validate_rules(options: &RuntimeOptions) -> structure::StructureResult<()> {
    let path = options
        .validate_rules_path
        .as_ref()
        .expect("rule validation path checked before dispatch");
    for line in validate_rule_file(path)? {
        println!("{line}");
    }
    Ok(())
}

fn validate_artifact(options: &RuntimeOptions) -> structure::StructureResult<()> {
    let path = options
        .validate_path
        .as_ref()
        .expect("validation path checked before dispatch");
    for line in validate_file(path)? {
        println!("{line}");
    }
    Ok(())
}

fn export_headless(options: &RuntimeOptions) -> structure::StructureResult<()> {
    let saved = generate_saved_structure_with_rules(
        options.seed.clone(),
        options.config.clone(),
        options.rule_packs.clone(),
    )?;
    std::fs::write(CURRENT_SEED_FILE, &options.seed)?;
    structure::save_structure(&options.export_path, &saved)?;
    if let Some(path) = &options.scenario_path {
        save_scenario(path, &generate_scenario(&saved))?;
    }
    Ok(())
}

fn inspect_saved_structure(options: &RuntimeOptions) -> structure::StructureResult<()> {
    let path = options
        .inspect_path
        .as_ref()
        .expect("inspect path checked before dispatch");
    let saved = structure::load_structure(path)?;
    if options.inspect_json {
        println!(
            "{}",
            render_inspection_json(&saved, &options.inspect_sections)?
        );
    } else {
        println!("{}", render_inspection(&saved, &options.inspect_sections));
    }
    Ok(())
}
