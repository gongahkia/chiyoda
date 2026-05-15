use gibson_rust::cli::{usage, RuntimeOptions};
use gibson_rust::generation::generate_saved_structure;
use gibson_rust::inspect::render_inspection;
use gibson_rust::structure::{self, CURRENT_SEED_FILE};

fn main() {
    let options = match RuntimeOptions::from_env() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };

    if options.inspect_path.is_some() {
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

fn export_headless(options: &RuntimeOptions) -> structure::StructureResult<()> {
    let saved = generate_saved_structure(options.seed.clone(), options.config.clone())?;
    std::fs::write(CURRENT_SEED_FILE, &options.seed)?;
    structure::save_structure(&options.export_path, &saved)
}

fn inspect_saved_structure(options: &RuntimeOptions) -> structure::StructureResult<()> {
    let path = options
        .inspect_path
        .as_ref()
        .expect("inspect path checked before dispatch");
    let saved = structure::load_structure(path)?;
    println!("{}", render_inspection(&saved, &options.inspect_sections));
    Ok(())
}
