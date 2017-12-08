extern crate game;

use std::io;
use std::error::Error;
use std::{thread, time};
use std::rc::Rc;

use game::config;
use game::resource;
use game::state::GameState;
use game::ui;

fn main() {
    let stdout = io::stdout();
    let stdin = io::stdin();
    let stdout = stdout.lock();
    let stdin = stdin.lock();

    let config = config::Config::new("config.json");
    let config = match config {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            eprintln!("There was a fatal error loading the configuration from 'config.json'");
            eprintln!("Exiting...");
            ::std::process::exit(1);
        }
    };

    let resource_set = resource::ResourceSet::new(&config.resources.directory);
    let resource_set = match resource_set {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  {}: {}", e.description(), e);
            eprintln!("There was a fatal error loading resource set from 'data':");
            eprintln!("Exiting...");
            ::std::process::exit(1);
        }
    };

    let mut io = game::io::create(config.display.adapter, stdin, stdout);
    io.init(&config);

    let frame_rate = config.display.frame_rate;

    let game_state = GameState::new(config, &resource_set);
    let mut game_state = match game_state {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}",  e);
            eprintln!("There was a fatal error creating the game state.");
            eprintln!("Exiting...");
            ::std::process::exit(1);
        }
    };

    let root = ui::create_ui_tree(Rc::clone(&game_state.area_state), &io);

    let fpms = (1000.0 / (frame_rate as f32)) as u64;
    let frame_time = time::Duration::from_millis(fpms);

    loop {
        let start_time = time::Instant::now();

        io.process_input(&mut game_state, root.borrow_mut());
        game_state.update();
        io.render_output(&game_state, root.borrow());

        if game_state.should_exit {
            break;
        }

        let elapsed = start_time.elapsed();
        if frame_time > elapsed {
            thread::sleep(frame_time - elapsed);
        }
    }
}