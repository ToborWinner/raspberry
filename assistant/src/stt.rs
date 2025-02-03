use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleRate, Stream,
};
use std::{sync::mpsc, time::Instant};
use thiserror::Error;
use vosk::{DecodingState, Model, Recognizer};

/// STTConfig can be used to configure the speech-to-text recognizer. It can be created by calling
/// [STTConfig::build]. The recognizer can be used by calling [STTSentenceRecognizer::recognize].
pub struct STTConfig {
    stream_config: cpal::StreamConfig,
    input_device: cpal::Device,
}

#[derive(Error, Debug)]
pub enum STTConfigError {
    #[error("Failed to get default input device")]
    FailedGetDefaultInputDevice,
    #[error("Failed to get default input config")]
    FailedGetDefaultInputConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("Failed to list input configs")]
    FailedListInputConfigs(#[from] cpal::SupportedStreamConfigsError),
    #[error("Failed to get a supported input config")]
    FailedGetSupportedInputConfig,
}

impl STTConfig {
    pub fn build() -> Result<Self, STTConfigError> {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .ok_or(STTConfigError::FailedGetDefaultInputDevice)?;

        let default_input_config = input_device.default_input_config()?;

        let input_config = if default_input_config.sample_format() == cpal::SampleFormat::I16
            && default_input_config.channels() == 1
        {
            default_input_config
        } else {
            // look for any compatible configuration
            input_device
                .supported_input_configs()?
                .find(|sc| {
                    sc.sample_format() == cpal::SampleFormat::I16
                        && sc.channels() == 1
                        && sc.min_sample_rate().0 <= 16000
                        && 16000 <= sc.max_sample_rate().0
                })
                .map(|sc| sc.with_sample_rate(SampleRate(16000)))
                .ok_or(STTConfigError::FailedGetSupportedInputConfig)?
        };

        let stream_config = cpal::StreamConfig {
            channels: input_config.channels(),
            sample_rate: input_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        Ok(STTConfig {
            stream_config,
            input_device,
        })
    }
}

#[derive(Error, Debug)]
#[error("Failed to load STT model")]
pub struct STTLoadModelFail;

/// Load the Vosk model. This function will return an error if the model fails to load.
/// Loading the model might take some time, so it is recommended to call this function once and
/// reuse the model.
pub fn load_stt_model(path: impl Into<String>) -> Result<Model, STTLoadModelFail> {
    Model::new(path).ok_or(STTLoadModelFail)
}

#[derive(Debug)]
pub enum RecognitionResult {
    Final(String),
    Failed,
    Cancelled,
}

#[derive(Error, Debug)]
pub enum RecognitionError {
    #[error("Failed to create recognizer")]
    FailedCreateRecognizer,
    #[error("Failed to receive recognition result")]
    FailedReceiveResult,
    #[error("Failed to play stream")]
    FailedPlayStream(#[from] cpal::PlayStreamError),
}

/// STTSentenceRecognizer is used to recognize a sentence from the microphone. It can be created by
/// calling [STTSentenceRecognizer::new]. The sentence can be recognized by calling
/// [STTSentenceRecognizer::recognize], which will block until the sentence is recognized. Timeout
/// is set to 20 seconds.
pub struct STTSentenceRecognizer<'a> {
    model: &'a Model,
    config: &'a STTConfig,
}

impl<'a> STTSentenceRecognizer<'a> {
    pub fn new(model: &'a Model, config: &'a STTConfig) -> Self {
        STTSentenceRecognizer { model, config }
    }

    pub fn recognize(self) -> Result<RecognitionResult, RecognitionError> {
        let recognizer =
            Recognizer::new(self.model, 16000.).ok_or(RecognitionError::FailedCreateRecognizer)?;

        let (tx, rx) = mpsc::channel();
        let stream = init_stream(
            &self.config.input_device,
            &self.config.stream_config,
            tx,
            recognizer,
        );
        stream.play()?;

        let result = rx
            .recv()
            .map_err(|_| RecognitionError::FailedReceiveResult)?;
        drop(stream);
        Ok(result)
    }
}

fn init_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: mpsc::Sender<RecognitionResult>,
    mut recognizer: Recognizer,
) -> Stream {
    let start_time = Instant::now();

    let error_callback = move |err| {
        eprintln!("an error occurred on stream: {}", err);
    };

    let data_callback = move |data: &[i16], _: &_| match recognizer.accept_waveform(data).unwrap() {
        DecodingState::Finalized => {
            tx.send(RecognitionResult::Final(
                recognizer.result().single().unwrap().text.to_string(),
            ))
            .unwrap();
        }
        DecodingState::Failed => tx.send(RecognitionResult::Failed).unwrap(),
        DecodingState::Running => {
            if start_time.elapsed().as_secs() > 20 {
                tx.send(RecognitionResult::Cancelled).unwrap();
            }
        }
    };
    device
        .build_input_stream::<i16, _, _>(&config, data_callback, error_callback, None)
        .expect("Failed to build input stream")
}
