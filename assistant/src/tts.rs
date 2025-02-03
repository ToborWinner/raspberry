use tts::{Backends, Tts};

pub use tts::Error as TtsError;

pub fn get_tts() -> Result<Tts, TtsError> {
    let mut tts = Tts::new(Backends::SpeechDispatcher)?;
    // let voices = tts.voices()?;
    // tts.set_voice(&voices[0])?;
    tts.set_rate(0.0)?;
    Ok(tts)
}

pub fn tts_speak(tts: &mut Tts, text: impl Into<String>) -> Result<(), TtsError> {
    tts.speak(text, true).map(|_| ())
}
