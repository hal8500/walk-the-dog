#![allow(unused)]
use std::{collections::HashMap, ops::Index};

use crate::{
    browser,
    engine::{self, Game, KeyState, Point, Rect, Renderer},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::Deserialize;
use web_sys::HtmlImageElement;

#[derive(Deserialize, Clone, Copy)]
struct SheetRect {
    x: i16,
    y: i16,
    w: i16,
    h: i16,
}

impl From<SheetRect> for Rect {
    fn from(value: SheetRect) -> Self {
        Self {
            x: value.x.into(),
            y: value.y.into(),
            width: value.w.into(),
            height: value.h.into(),
        }
    }
}

#[derive(Deserialize, Clone)]
struct Cell {
    frame: SheetRect,
}

#[derive(Deserialize, Clone)]
struct Sheet {
    frames: HashMap<String, Cell>,
}

#[derive(Clone)]
struct AnimationSprite {
    cells: Vec<Rect>,
}

impl AnimationSprite {
    fn new(name: &'static str, sheet: &Sheet) -> Self {
        let mut cells: Vec<Rect> = vec![];
        let mut i = 1;
        loop {
            let frame_name = format!("{} ({}).png", name, i);
            i += 1;
            if let Some(cell) = sheet.frames.get(&frame_name) {
                cells.push(cell.frame.into());
            } else {
                break;
            }
        }
        Self { cells }
    }

    fn get(&self, frame: u8) -> Option<&Rect> {
        let idx = frame as usize / 3;
        self.cells.get(idx)
    }

    fn len_frames(&self) -> u8 {
        (self.cells.len() as u8) * 3
    }
}

#[derive(Debug, Clone, Copy)]
enum BlueHatBoyState {
    Idle,
    Running,
    Sliding,
    Jumping,
}

const NUM_BHB_SATES: usize = 4;

impl From<BlueHatBoyState> for usize {
    fn from(value: BlueHatBoyState) -> Self {
        match value {
            BlueHatBoyState::Idle => 0,
            BlueHatBoyState::Running => 1,
            BlueHatBoyState::Sliding => 2,
            BlueHatBoyState::Jumping => 3,
        }
    }
}

impl From<usize> for BlueHatBoyState {
    fn from(value: usize) -> Self {
        match value {
            1 => BlueHatBoyState::Running,
            2 => BlueHatBoyState::Sliding,
            3 => BlueHatBoyState::Jumping,
            _ => BlueHatBoyState::Idle,
        }
    }
}

impl BlueHatBoyState {
    fn frame_name(self) -> &'static str {
        ["Idle", "Run", "Slide", "Jump"][usize::from(self)]
    }
    fn all() -> [BlueHatBoyState; NUM_BHB_SATES] {
        [
            BlueHatBoyState::Idle,
            BlueHatBoyState::Running,
            BlueHatBoyState::Sliding,
            BlueHatBoyState::Jumping,
        ]
    }
}

impl<T> Index<BlueHatBoyState> for [T] {
    type Output = T;
    fn index(&self, index: BlueHatBoyState) -> &Self::Output {
        &self[usize::from(index)]
    }
}

const FLOOR: i16 = 475;
const RUNNING_SPEED: i16 = 1;
const JUMP_SPEED: i16 = -25;
const GRAVITY: i16 = 1;

pub struct BlueHatBoy {
    state: BlueHatBoyState,
    animations: [AnimationSprite; NUM_BHB_SATES],
    image: HtmlImageElement,
    frame: u8,
    position: Point,
    velocity: Point,
}

impl BlueHatBoy {
    fn new(sheet: Sheet, image: HtmlImageElement) -> Self {
        let animations =
            BlueHatBoyState::all().map(|s| AnimationSprite::new(s.frame_name(), &sheet));

        BlueHatBoy {
            state: BlueHatBoyState::Idle,
            animations,
            image,
            frame: 0,
            position: Point { x: 0, y: FLOOR },
            velocity: Point::default(),
        }
    }
    fn run_right(&mut self) {
        if let BlueHatBoyState::Idle = self.state {
            self.velocity.x = RUNNING_SPEED;
            self.frame = 0;
            self.state = BlueHatBoyState::Running;
        }
    }
    fn slide(&mut self) {
        if let BlueHatBoyState::Running = self.state {
            self.frame = 0;
            self.state = BlueHatBoyState::Sliding;
        }
    }
    fn jump(&mut self) {
        if let BlueHatBoyState::Running = self.state {
            self.frame = 0;
            self.velocity.y = JUMP_SPEED;
            self.state = BlueHatBoyState::Jumping;
        }
    }

    fn update(&mut self) {
        self.frame += 1;

        match self.state {
            BlueHatBoyState::Sliding => {
                if self.frame == self.get_sprite().len_frames() {
                    self.frame = 0;
                    self.state = BlueHatBoyState::Running;
                }
            }
            BlueHatBoyState::Jumping => {
                if FLOOR <= self.position.y {
                    self.frame = 0;
                    self.state = BlueHatBoyState::Running;
                }
            }
            _ => (),
        }

        if self.frame >= self.get_sprite().len_frames() {
            self.frame = 0;
        }
        self.velocity.y += GRAVITY;
        self.position.x += self.velocity.x;
        self.position.y += self.velocity.y;
        if self.position.y > FLOOR {
            self.position.y = FLOOR;
            self.velocity.y = 0;
        }
    }

    fn draw(&self, renderer: &Renderer) {
        let frame = self.get_sprite().get(self.frame).unwrap();
        let pos = &self.position;
        renderer.draw_image(
            &self.image,
            &frame,
            &Rect {
                x: pos.x.into(),
                y: pos.y.into(),
                width: frame.width,
                height: frame.height,
            },
        );
    }

    fn get_sprite(&self) -> &AnimationSprite {
        &self.animations[self.state]
    }
}

pub enum WalkTheDog {
    Loading,
    Loaded(BlueHatBoy),
}

impl WalkTheDog {
    pub fn new() -> Self {
        WalkTheDog::Loading
    }
}

#[async_trait(?Send)]
impl Game for WalkTheDog {
    async fn initialize(&self) -> Result<Box<dyn Game>> {
        match self {
            WalkTheDog::Loading => {
                let json = browser::fetch_json("rhb.json").await?;
                let sheet: Sheet = serde_wasm_bindgen::from_value(json)
                    .map_err(|_| anyhow!("Could not convert rhb.json into a Sheet structure"))?;
                let image = engine::load_image("rhb.png").await?;
                let rhb = BlueHatBoy::new(sheet, image);

                Ok(Box::new(WalkTheDog::Loaded(rhb)))
            }
            WalkTheDog::Loaded(_) => Err(anyhow!("Error: Game is already initialized!")),
        }
    }

    fn update(&mut self, keystate: &KeyState) {
        if let WalkTheDog::Loaded(rhb) = self {
            if keystate.is_pressed("ArrowDown") {
                rhb.slide();
            }
            if keystate.is_pressed("ArrowRight") {
                rhb.run_right();
            }
            if keystate.is_pressed("Space") {
                rhb.jump();
            }

            rhb.update();
        }
    }
    fn draw(&self, renderer: &Renderer) {
        renderer.clear(&Rect {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 600.0,
        });
        if let WalkTheDog::Loaded(rhb) = self {
            rhb.draw(renderer);
        }
    }
}
