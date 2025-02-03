use std::{collections::HashSet, sync::mpsc::RecvError};

use ::tts::Tts;
use intents::{
    EmbeddingModelSource, IntentRecognizer, IntentRecognizerBuildError, IntentRecognizerError,
    IntentsConfig,
};
use stt::{
    load_stt_model, RecognitionError, RecognitionResult, STTConfig, STTConfigError,
    STTSentenceRecognizer,
};
use thiserror::Error;
use tts::{tts_speak, TtsError};
use vosk::Model;
use wakeword::{
    WakewordConfig, WakewordConfigAddError, WakewordConfigBuildError, WakewordConfigStartError,
};

pub mod intents;
pub mod stt;
pub mod tts;
pub mod wakeword;

pub struct AssistantConfig<T> {
    wakeword_config: WakewordConfig,
    stt_model: Model,
    stt_config: STTConfig,
    tts: Tts,
    intents_config: IntentsConfig<T>,
    wakewords_listen: HashSet<String>,
}

#[derive(Error, Debug)]
pub enum AssistantConfigBuildError {
    #[error("Failed to build wakeword config")]
    WakewordConfigError(#[from] WakewordConfigBuildError),
    #[error("Failed to load STT model")]
    STTModelError,
    #[error("Failed to build STT config")]
    STTConfigError(#[from] STTConfigError),
    #[error("Failed to get TTS")]
    TtsError(#[from] TtsError),
}

#[derive(Error, Debug)]
pub enum AssistantStartError {
    #[error("Failed to build intent recognizer")]
    IntentRecognizerBuildError(#[from] IntentRecognizerBuildError),
    #[error("Failed to start wakeword listener")]
    WakewordListenerStartError(#[from] WakewordConfigStartError),
}

impl<T> AssistantConfig<T> {
    pub fn build(
        stt_model_path: impl Into<String>,
        embedding_model: EmbeddingModelSource,
    ) -> Result<Self, AssistantConfigBuildError> {
        let wakeword_config = WakewordConfig::build()?;
        let stt_model =
            load_stt_model(stt_model_path).map_err(|_| AssistantConfigBuildError::STTModelError)?;
        let stt_config = STTConfig::build()?;
        let tts = tts::get_tts()?;
        let intents_config = IntentsConfig::new(embedding_model);

        Ok(Self {
            wakeword_config,
            stt_model,
            stt_config,
            tts,
            intents_config,
            wakewords_listen: HashSet::new(),
        })
    }

    pub fn add_wakeword_from_file(
        &mut self,
        wakeword: &str,
        file: &str,
        listen: bool,
    ) -> Result<(), WakewordConfigAddError> {
        self.wakeword_config
            .add_wakeword_from_file(wakeword, file)?;
        if listen {
            self.wakewords_listen.insert(wakeword.to_string());
        }
        Ok(())
    }

    pub fn add_intent(&mut self, id: T, examples: Vec<String>) {
        self.intents_config.add_intent(id, examples);
    }

    pub fn start(self) -> Result<Assistant<T>, AssistantStartError> {
        let intent_recognizer = IntentRecognizer::build(self.intents_config)?;
        let wakeword_listener = self.wakeword_config.start()?;

        Ok(Assistant {
            stt_model: self.stt_model,
            stt_config: self.stt_config,
            tts: self.tts,
            intent_recognizer,
            wakeword_listener,
            wakewords_listen: self.wakewords_listen,
        })
    }
}

#[derive(Error, Debug)]
pub enum AssistantListenSuccessfulWakewordError {
    #[error("Error while initializing speech recognition")]
    SpeechRecognitionInitializationError(#[from] RecognitionError),
    #[error("Failed to recognize speech")]
    SpeechRecognitionError,
    #[error("Speech recognition timed out")]
    SpeechRecognitionTimeout,
    #[error("Failed to recognize intent")]
    IntentRecognizerError(#[from] IntentRecognizerError),
}

#[derive(Error, Debug)]
pub enum AssistantListenError {
    #[error("Failed to receive wakeword")]
    WakewordRecvError(#[from] RecvError),
    #[error("Something went wrong while processing data after wakeword detection")]
    ProcessError(String, AssistantListenSuccessfulWakewordError),
}

pub struct Assistant<T> {
    stt_model: Model,
    stt_config: STTConfig,
    tts: Tts,
    intent_recognizer: IntentRecognizer<T>,
    wakeword_listener: wakeword::WakewordListener,
    wakewords_listen: HashSet<String>,
}

impl<T> Assistant<T> {
    pub fn listen(&self) -> Result<AssistantQuery<T>, AssistantListenError> {
        let wakeword = self.wakeword_listener.listen()?;
        match self.tts.is_speaking() {
            Err(_) => {
                return Err(AssistantListenError::ProcessError(
                    wakeword,
                    AssistantListenSuccessfulWakewordError::SpeechRecognitionError,
                ))
            }
            Ok(true) => {
                return {
                    _ = self.finish_speaking();
                    self.listen()
                }
            }
            Ok(false) => (),
        }

        if !self.wakewords_listen.contains(&wakeword) {
            return Ok(AssistantQuery {
                wakeword,
                intent: None,
            });
        }

        let recognizer = STTSentenceRecognizer::new(&self.stt_model, &self.stt_config);

        let result = recognizer
            .recognize()
            .map_err(|e| AssistantListenError::ProcessError(wakeword.clone(), e.into()))?;

        let text = match result {
            RecognitionResult::Final(text) => text,
            RecognitionResult::Failed => {
                return Err(AssistantListenError::ProcessError(
                    wakeword,
                    AssistantListenSuccessfulWakewordError::SpeechRecognitionError,
                ))
            }
            RecognitionResult::Cancelled => {
                return Err(AssistantListenError::ProcessError(
                    wakeword,
                    AssistantListenSuccessfulWakewordError::SpeechRecognitionTimeout,
                ))
            }
        };

        let intent = self
            .intent_recognizer
            .recognize(&text)
            .map_err(|e| AssistantListenError::ProcessError(wakeword.clone(), e.into()))?;

        Ok(AssistantQuery {
            wakeword,
            intent: Some(intent),
        })
    }

    pub fn speak(&mut self, text: impl Into<String>) -> Result<(), TtsError> {
        tts_speak(&mut self.tts, text)
    }

    pub fn finish_speaking(&self) -> Result<(), TtsError> {
        while self.tts.is_speaking()? {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(())
    }
}

pub struct AssistantQuery<'a, T> {
    pub wakeword: String,
    pub intent: Option<&'a T>,
}
