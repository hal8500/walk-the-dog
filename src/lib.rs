#[macro_use]
mod browser;
mod engine;
mod game;
mod miya;
mod segments;
mod sound;
mod utils;
use engine::GameLoop;
use game::WalkTheDog;
use utils::set_logs;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[wasm_bindgen(start)]
pub fn main_js() -> Result<(), JsValue> {
    set_logs();

    browser::spawn_local(async move {
        let game = WalkTheDog::new();
        GameLoop::start(game)
            .await
            .expect("Could not start game loop");
    });

    Ok(())
}
