use assistant::{
    intents::{
        EmbeddingModelFilePaths, EmbeddingModelSource, InitOptionsUserDefined,
        IntentRecognizerError,
    },
    AssistantConfig, AssistantListenError, AssistantListenSuccessfulWakewordError,
};
use chrono::Local;
use dirs::{get_config_file, get_config_path};

mod dirs;
mod scheduler;

macro_rules! speak {
    ($assistant:expr, $content:expr) => {
        $assistant.speak($content).expect("Failed to speak.")
    };
}

enum Intents {
    Greeting,
    Weather,
    Time,
    Day,
    Date,
}

fn main() {
    let config_dir = get_config_path();
    let mut config = AssistantConfig::build(get_config_file(&config_dir, "vosk-model-small-en-us-0.15").to_str().expect("Failed to convert PathBuf to &str"), EmbeddingModelSource::Local(EmbeddingModelFilePaths {
        onnx: get_config_file(&config_dir, "intents/model.onnx").to_str().expect("Failed to convert PathBuf to &str"),
        tokenizer: get_config_file(&config_dir, "intents/tokenizer.json").to_str().expect("Failed to convert PathBuf to &str"),
        config: get_config_file(&config_dir, "intents/config.json").to_str().expect("Failed to convert PathBuf to &str"),
        special_tokens_map: get_config_file(&config_dir, "intents/special_tokens_map.json").to_str().expect("Failed to convert PathBuf to &str"),
        tokenizer_config: get_config_file(&config_dir, "intents/tokenizer_config.json").to_str().expect("Failed to convert PathBuf to &str"),
    }.to_user_defined_embedding_model().expect("Couldn't find model files for intent recognition"), InitOptionsUserDefined::new())).expect("Failed to build assistant config. Please ensure you have all required files setup in the correct location.");

    config
        .add_wakeword_from_file(
            "pizza",
            get_config_file(&config_dir, "pizza.rpw")
                .to_str()
                .expect("Failed to convert PathBuf to &str"),
            true,
        )
        .expect("Failed to add wakeword, are you sure it's valid?");
    config.add_intent(
        Intents::Greeting,
        vec!["hello".to_string(), "hi".to_string(), "hey".to_string()],
    );
    config.add_intent(
        Intents::Weather,
        vec![
            "what's the weather like today".to_string(),
            "what's the forecast".to_string(),
        ],
    );
    config.add_intent(
        Intents::Time,
        vec![
            "what time is it".to_string(),
            "what's the current time".to_string(),
        ],
    );
    config.add_intent(
        Intents::Day,
        vec![
            "what day is it".to_string(),
            "what's the current day".to_string(),
        ],
    );
    config.add_intent(
        Intents::Date,
        vec![
            "what's the date".to_string(),
            "what's today's date".to_string(),
        ],
    );

    let mut assistant = config.start().expect("Failed to start assistant");

    println!("Listening for wakewords...");
    loop {
        let query = match assistant.listen() {
            Ok(query) => query,
            Err(AssistantListenError::WakewordRecvError(e)) => {
                eprintln!("Stream shut down, failed to receive wakeword. Error: {}", e);
                break;
            }
            Err(AssistantListenError::ProcessError(_, e)) => {
                match e {
                AssistantListenSuccessfulWakewordError::SpeechRecognitionInitializationError(
                    e_in,
                ) => {
                    eprintln!("Failed to initialize speech recognition: {:?}", e_in);
                    speak!(assistant, "Failed to initialize speech recognition. Please try again.");
                }
                AssistantListenSuccessfulWakewordError::SpeechRecognitionError => {
                    eprintln!("Failed to recognize speech.");
                    speak!(assistant, "Failed to recognize speech. Please try again.");
                }
                AssistantListenSuccessfulWakewordError::SpeechRecognitionTimeout => speak!(assistant, "You took too long to speak, sorry. Please try again."),
                AssistantListenSuccessfulWakewordError::IntentRecognizerError(IntentRecognizerError::TextEmbeddingError(e_in)) => {
                    eprintln!("Failed to embed text: {:?}", e_in);
                    speak!(assistant, "There was a problem with the intent recognizer. Please try again.");
                }
                AssistantListenSuccessfulWakewordError::IntentRecognizerError(IntentRecognizerError::ScoreTooLow) => speak!(assistant, "I'm not sure I can do that, sorry."),
            };
                continue;
            }
        };

        match query
            .intent
            .expect("Only added wakewords that listen, so should not happen")
        {
            Intents::Greeting => speak!(assistant, "Hello! How can I help you today?"),
            Intents::Weather => speak!(assistant, "I'm sorry, but I can't fetch the weather yet."),
            Intents::Time => speak!(
                assistant,
                format!("It's {}.", Local::now().format("%I:%M:%S %p"))
            ),
            Intents::Day => speak!(assistant, format!("It's {}.", Local::now().format("%A"))),
            Intents::Date => speak!(
                assistant,
                format!("It's {}.", Local::now().format("%B %d, %Y"))
            ),
        }
    }
}
