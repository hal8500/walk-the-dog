use anyhow::{anyhow, Result};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    js_sys::ArrayBuffer, AudioBuffer, AudioBufferSourceNode, AudioContext, AudioDestinationNode,
    AudioNode,
};
pub enum Looping {
    No,
    Yes,
}

pub fn create_audio_context() -> Result<AudioContext> {
    AudioContext::new().map_err(|err| anyhow!("Counld not create audio context: {:#?}", err))
}

fn create_buffer_source(ctx: &AudioContext) -> Result<AudioBufferSourceNode> {
    ctx.create_buffer_source()
        .map_err(|err| anyhow!("Error creating buffer source: {:#?}", err))
}

fn connect_with_param_audio_node(
    ctx: &AudioContext,
    volume: f32,
    buffer_source: &AudioBufferSourceNode,
    destination: &AudioDestinationNode,
) -> Result<AudioNode> {
    let g = ctx.create_gain().unwrap();
    g.gain().set_value(volume);

    buffer_source.connect_with_audio_node(&g).unwrap();
    g.connect_with_audio_node(destination)
        .map_err(|err| anyhow!("Error connecting audio source to destination {:#?}", err))
}

fn create_track_sound(
    ctx: &AudioContext,
    buffer: &AudioBuffer,
    volume: f32,
) -> Result<AudioBufferSourceNode> {
    let track_source = create_buffer_source(ctx)?;
    track_source.set_buffer(Some(buffer));
    connect_with_param_audio_node(ctx, volume, &track_source, &ctx.destination())?;
    Ok(track_source)
}

pub fn play_sound(
    ctx: &AudioContext,
    buffer: &AudioBuffer,
    looping: Looping,
    volume: f32,
) -> Result<()> {
    let track_source = create_track_sound(ctx, buffer, volume)?;
    if matches!(looping, Looping::Yes) {
        track_source.set_loop(true);
    }
    track_source
        .start()
        .map_err(|err| anyhow!("Could not start sound! {:#?}", err))
}

pub async fn decode_audio_data(
    ctx: &AudioContext,
    array_buffer: &ArrayBuffer,
) -> Result<AudioBuffer> {
    JsFuture::from(
        ctx.decode_audio_data(array_buffer)
            .map_err(|err| anyhow!("Could not decode audio from array buffer {:#?}", err))?,
    )
    .await
    .map_err(|err| anyhow!("Could not convert promise to future {:#?}", err))?
    .dyn_into()
    .map_err(|err| anyhow!("Could not cast into AudioBuffer {:#?}", err))
}
