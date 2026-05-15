use gibson_rust::seed::{generate_seed, validate_seed};

fn window_conf() -> macroquad::prelude::Conf {
    gibson_rust::render::window_conf()
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = if args.len() > 1 {
        let candidate = args[1].to_uppercase();
        if !validate_seed(&candidate) {
            eprintln!("Invalid seed '{}'. Must be 8 alphanumeric chars.", args[1]);
            std::process::exit(1);
        }
        candidate
    } else {
        generate_seed()
    };

    gibson_rust::render::run(seed).await;
}
