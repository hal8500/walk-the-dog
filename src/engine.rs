use std::{cell::RefCell, collections::HashMap, fmt::Debug, rc::Rc, sync::Mutex};

use crate::{
    browser::{self, LoopClosure},
    sound,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::channel::{
    mpsc::{unbounded, UnboundedReceiver},
    oneshot::channel,
};
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{AudioBuffer, AudioContext, CanvasRenderingContext2d, HtmlElement, HtmlImageElement};

const FRAME_SIZE: f32 = 1.0 / 60.0 * 1000.0;
type SharedLoopClosure = Rc<RefCell<Option<LoopClosure>>>;

#[derive(Debug, Clone, Copy, Default)]
pub struct Point {
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub position: Point,
    pub width: i16,
    pub height: i16,
}

impl Rect {
    pub const fn new(position: Point, width: i16, height: i16) -> Self {
        Rect {
            position,
            width,
            height,
        }
    }

    pub const fn new_from_x_y(x: i16, y: i16, width: i16, height: i16) -> Self {
        Rect {
            position: Point { x, y },
            width,
            height,
        }
    }

    pub fn x(&self) -> i16 {
        self.position.x
    }

    pub fn y(&self) -> i16 {
        self.position.y
    }

    pub fn set_x(&mut self, x: i16) {
        self.position.x = x;
    }

    pub fn intersects(&self, rect: &Rect) -> bool {
        self.x() < rect.right()
            && self.right() > rect.x()
            && self.y() < rect.bottom()
            && self.bottom() > rect.y()
    }

    pub fn right(&self) -> i16 {
        self.x() + self.width
    }

    pub fn bottom(&self) -> i16 {
        self.y() + self.height
    }
}

#[derive(Deserialize, Clone, Copy)]
pub struct SheetRect {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
}

impl From<SheetRect> for Rect {
    fn from(value: SheetRect) -> Self {
        Self::new_from_x_y(value.x, value.y, value.w, value.h)
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub frame: SheetRect,
    pub sprite_source_size: SheetRect,
}

#[derive(Deserialize, Clone)]
pub struct Sheet {
    pub frames: HashMap<String, Cell>,
}

pub struct SpriteSheet {
    sheet: Sheet,
    image: HtmlImageElement,
}

impl SpriteSheet {
    pub fn new(sheet: Sheet, image: HtmlImageElement) -> Self {
        Self { sheet, image }
    }

    pub fn cell(&self, name: &str) -> Option<&Cell> {
        self.sheet.frames.get(name)
    }

    pub fn draw(&self, renderer: &Renderer, source: &Rect, destination: &Rect) {
        renderer.draw_image(&self.image, source, destination);
    }
}

pub struct Image {
    element: HtmlImageElement,
    bounding_box: Rect,
}

impl Image {
    pub fn new(element: HtmlImageElement, position: Point) -> Self {
        let bounding_box = Rect::new(position, element.width() as i16, element.height() as i16);
        Self {
            element,
            bounding_box,
        }
    }

    pub fn draw(&self, renderer: &Renderer) {
        renderer.draw_entire_image(&self.element, &self.bounding_box.position);
        if cfg!(feature = "draw_debug_info") {
            renderer.draw_rect(&self.bounding_box);
        }
    }

    pub fn bounding_box(&self) -> &Rect {
        &self.bounding_box
    }

    pub fn move_horizontally(&mut self, distance: i16) {
        self.set_x(self.bounding_box.x() + distance);
    }

    pub fn set_x(&mut self, x: i16) {
        self.bounding_box.set_x(x);
    }

    pub fn right(&self) -> i16 {
        self.bounding_box.right()
    }
}

#[async_trait(?Send)]
pub trait Game {
    async fn initialize(&self) -> Result<Box<dyn Game>>;
    fn update(&mut self, keystate: &KeyState);
    fn draw(&self, renderer: &Renderer);
}

pub async fn load_image(source: &str) -> Result<HtmlImageElement> {
    let image = browser::new_image()?;
    let (complete_tx, complete_rx) = channel::<Result<()>>();
    let success_tx = Rc::new(Mutex::new(Some(complete_tx)));
    let error_tx = Rc::clone(&success_tx);
    let success_callback = browser::closure_once(move || {
        if let Some(tx) = success_tx.lock().ok().and_then(|mut opt| opt.take()) {
            let _ = tx.send(Ok(()));
        }
    });
    let error_callback = browser::closure_once(move |err: JsValue| {
        if let Some(tx) = error_tx.lock().ok().and_then(|mut opt| opt.take()) {
            let _ = tx.send(Err(anyhow!("Error Loading Image: {:#?}", err)));
        }
    });

    image.set_onload(Some(success_callback.as_ref().unchecked_ref()));
    image.set_onerror(Some(error_callback.as_ref().unchecked_ref()));
    image.set_src(source);
    complete_rx.await??;
    Ok(image)
}

pub struct GameLoop {
    last_frame: f64,
    accumulated_delta: f32,
}

impl GameLoop {
    pub async fn start(game: impl Game + 'static) -> Result<()> {
        let mut keyevent_receiver = prepare_input()?;
        let mut game = game.initialize().await?;
        let mut game_loop = GameLoop {
            last_frame: browser::now()?,
            accumulated_delta: 0.0,
        };

        let renderer = Renderer {
            context: browser::context()?,
        };

        let f: SharedLoopClosure = Rc::new(RefCell::new(None));
        let g = f.clone();

        let mut keystate = KeyState::new();
        *g.borrow_mut() = Some(browser::create_raf_closure(move |pref: f64| {
            let frame_time = (pref - game_loop.last_frame) as f32;

            if game_loop.accumulated_delta + frame_time > FRAME_SIZE {
                game_loop.accumulated_delta += frame_time;
                game_loop.last_frame = pref;
                process_input(&mut keystate, &mut keyevent_receiver);

                while game_loop.accumulated_delta > FRAME_SIZE {
                    game.update(&keystate);
                    game_loop.accumulated_delta -= FRAME_SIZE;
                }

                game.draw(&renderer);

                if cfg!(feature = "draw_debug_info") {
                    unsafe {
                        draw_frame_rate(&renderer, frame_time);
                    }
                }
            }

            browser::request_animation_frame(f.borrow().as_ref().unwrap()).unwrap();
        }));

        browser::request_animation_frame(
            g.borrow()
                .as_ref()
                .ok_or_else(|| anyhow!("GameLoop: Loop is None"))?,
        )?;
        Ok(())
    }
}

#[cfg(feature = "draw_debug_info")]
unsafe fn draw_frame_rate(renderer: &Renderer, frame_time: f32) {
    static mut FRAMES_COUNTED: i32 = 0;
    static mut TOTAL_FRAME_TIME: f32 = 0.0;
    static mut FRAME_RATE: i32 = 0;

    FRAMES_COUNTED += 1;
    TOTAL_FRAME_TIME += frame_time;

    if TOTAL_FRAME_TIME > 1000.0 {
        FRAME_RATE = FRAMES_COUNTED;
        TOTAL_FRAME_TIME = 0.0;
        FRAMES_COUNTED = 0;
    }

    let fr = FRAME_RATE;
    if let Err(err) = renderer.draw_text(&format!("Frame Rate {}", fr), &Point { x: 400, y: 100 }) {
        log::error!("Could not draw text {:#?}", err);
    }
}

pub struct Renderer {
    context: CanvasRenderingContext2d,
}

impl Renderer {
    pub fn clear(&self, rect: &Rect) {
        self.context.clear_rect(
            rect.x().into(),
            rect.y().into(),
            rect.width.into(),
            rect.height.into(),
        );
    }

    pub fn draw_image(&self, image: &HtmlImageElement, frame: &Rect, destination: &Rect) {
        self.context
            .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                image,
                frame.x().into(),
                frame.y().into(),
                frame.width.into(),
                frame.height.into(),
                destination.x().into(),
                destination.y().into(),
                destination.width.into(),
                destination.height.into(),
            )
            .expect("Drawing is throwing exceptions! Unrecoverable error.");
    }

    pub fn draw_entire_image(&self, image: &HtmlImageElement, position: &Point) {
        self.context
            .draw_image_with_html_image_element(image, position.x.into(), position.y.into())
            .expect("Drawing is throwing exceptions! Unrecoverable error.");
    }

    #[cfg(feature = "draw_debug_info")]
    pub fn draw_rect(&self, bounding_box: &Rect) {
        self.context.set_stroke_style_str("#FF0000");
        self.context.begin_path();
        self.context.rect(
            bounding_box.x().into(),
            bounding_box.y().into(),
            bounding_box.width.into(),
            bounding_box.height.into(),
        );
        self.context.stroke();
    }

    #[cfg(feature = "draw_debug_info")]
    pub fn draw_text(&self, text: &str, location: &Point) -> Result<()> {
        self.context.set_font("16pt serif");
        self.context
            .fill_text(text, location.x.into(), location.y.into())
            .map_err(|err| anyhow!("Error filling text {:#?}", err))?;
        Ok(())
    }
}

enum KeyPress {
    KeyUp(web_sys::KeyboardEvent),
    KeyDown(web_sys::KeyboardEvent),
}

fn prepare_input() -> Result<UnboundedReceiver<KeyPress>> {
    let (ke_sender, ke_receiver) = unbounded();
    let kd_sender = Rc::new(RefCell::new(ke_sender));
    let ku_sender = Rc::clone(&kd_sender);

    let onkeydown = browser::closure_wrap(Box::new(move |keycode: web_sys::KeyboardEvent| {
        let _ = kd_sender
            .borrow_mut()
            .start_send(KeyPress::KeyDown(keycode));
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

    let onkeyup = browser::closure_wrap(Box::new(move |keycode: web_sys::KeyboardEvent| {
        let _ = ku_sender.borrow_mut().start_send(KeyPress::KeyUp(keycode));
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

    browser::window()?.set_onkeydown(Some(onkeydown.as_ref().unchecked_ref()));
    browser::window()?.set_onkeyup(Some(onkeyup.as_ref().unchecked_ref()));
    onkeydown.forget();
    onkeyup.forget();
    Ok(ke_receiver)
}

fn process_input(state: &mut KeyState, keyevent_receiver: &mut UnboundedReceiver<KeyPress>) {
    loop {
        match keyevent_receiver.try_next() {
            Ok(None) => break,
            Err(_err) => break,
            Ok(Some(evt)) => match evt {
                KeyPress::KeyUp(evt) => state.set_released(&evt.code()),
                KeyPress::KeyDown(evt) => state.set_pressed(&evt.code(), evt),
            },
        };
    }
}

pub struct KeyState {
    pressed_keys: HashMap<String, web_sys::KeyboardEvent>,
}

impl KeyState {
    fn new() -> Self {
        KeyState {
            pressed_keys: HashMap::new(),
        }
    }

    pub fn is_pressed(&self, code: &str) -> bool {
        self.pressed_keys.contains_key(code)
    }

    fn set_pressed(&mut self, code: &str, event: web_sys::KeyboardEvent) {
        self.pressed_keys.insert(code.into(), event);
    }

    fn set_released(&mut self, code: &str) {
        self.pressed_keys.remove(code);
    }
}

impl Debug for KeyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pressed: ")?;
        for p in self.pressed_keys.keys() {
            write!(f, "{}", p)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Audio {
    context: AudioContext,
}

#[derive(Clone)]
pub struct Sound {
    pub(crate) buffer: AudioBuffer,
}

impl Audio {
    pub fn new() -> Result<Self> {
        Ok(Self {
            context: sound::create_audio_context()?,
        })
    }

    pub async fn load_sound(&self, filename: &str) -> Result<Sound> {
        let array_buffer = browser::fetch_array_buffer(filename).await?;
        let audio_buffer = sound::decode_audio_data(&self.context, &array_buffer).await?;
        Ok(Sound {
            buffer: audio_buffer,
        })
    }

    pub fn play_sound(&self, sound: &Sound) -> Result<()> {
        sound::play_sound(&self.context, &sound.buffer, sound::Looping::No, 1.0)
    }

    pub fn play_looping_sound(&self, sound: &Sound) -> Result<()> {
        sound::play_sound(&self.context, &sound.buffer, sound::Looping::Yes, 0.001)
    }
}

pub fn add_click_handler(elem: HtmlElement) -> UnboundedReceiver<()> {
    let (mut click_sender, click_reciever) = unbounded();
    let on_click = browser::closure_wrap(Box::new(move || {
        let _ = click_sender.start_send(());
    }) as Box<dyn FnMut()>);
    elem.set_onclick(Some(on_click.as_ref().unchecked_ref()));
    on_click.forget();
    click_reciever
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_rects_that_intersect_on_the_left() {
        let rect1 = Rect {
            position: Point { x: 10, y: 10 },
            height: 100,
            width: 100,
        };

        let rect2 = Rect {
            position: Point { x: 0, y: 10 },
            height: 100,
            width: 100,
        };

        assert_eq!(rect2.intersects(&rect1), true);
    }
}
