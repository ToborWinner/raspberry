use std::{fs::read, io};

pub use fastembed::{
    InitOptions, InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use thiserror::Error;

pub struct IntentsConfig<T> {
    intents: Vec<Intent<T>>,
    model: EmbeddingModelSource,
}

struct Intent<T> {
    id: T,
    examples: Vec<String>,
}

impl<T> IntentsConfig<T> {
    pub fn new(model: EmbeddingModelSource) -> Self {
        Self {
            intents: Vec::new(),
            model,
        }
    }

    pub fn add_intent(&mut self, id: T, examples: Vec<String>) {
        self.intents.push(Intent { id, examples });
    }
}

pub enum EmbeddingModelSource {
    Online(InitOptions),
    Local(UserDefinedEmbeddingModel, InitOptionsUserDefined),
}

struct ProcessedIntent<T> {
    id: T,
    examples: Vec<Vec<f32>>,
}

pub struct IntentRecognizer<T> {
    intents: Vec<ProcessedIntent<T>>,
    model: TextEmbedding,
}

#[derive(Error, Debug)]
pub enum IntentRecognizerBuildError {
    #[error("Failed to build text embedding model")]
    TextEmbeddingError(#[from] fastembed::Error),
    #[error("No intents provided")]
    NoIntentsProvided,
}

#[derive(Error, Debug)]
pub enum IntentRecognizerError {
    #[error("Failed to embed text")]
    TextEmbeddingError(#[from] fastembed::Error),
    #[error("Failed to recognize intent, matched none with a high enough score")]
    ScoreTooLow,
}

impl<T> IntentRecognizer<T> {
    pub fn build(config: IntentsConfig<T>) -> Result<Self, IntentRecognizerBuildError> {
        if config.intents.is_empty() {
            return Err(IntentRecognizerBuildError::NoIntentsProvided);
        }

        let model = match config.model {
            EmbeddingModelSource::Online(config) => TextEmbedding::try_new(config),
            EmbeddingModelSource::Local(model, config) => {
                TextEmbedding::try_new_from_user_defined(model, config)
            }
        }?;

        Ok(Self {
            intents: config
                .intents
                .into_iter()
                .map(|intent| {
                    model
                        .embed(intent.examples, None)
                        .map(|examples| ProcessedIntent {
                            id: intent.id,
                            examples,
                        })
                })
                .collect::<Result<_, _>>()?,
            model,
        })
    }

    pub fn recognize(&self, text: &str) -> Result<&T, IntentRecognizerError> {
        let target = self
            .model
            .embed(vec![text], None)?
            .into_iter()
            .next()
            .unwrap();

        let (intent, score) = find_closest(&self.intents, target);

        if score < 0.5 {
            return Err(IntentRecognizerError::ScoreTooLow);
        }

        Ok(intent)
    }
}

fn compute_cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot_product / (magnitude_a * magnitude_b)
}

fn find_closest<T>(intents: &[ProcessedIntent<T>], target: Vec<f32>) -> (&T, f32) {
    intents
        .into_iter()
        .flat_map(|ProcessedIntent { id, examples }| examples.iter().map(move |e| (id, e)))
        .map(|(n, e)| (n, compute_cosine_distance(e, &target)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Less))
        .unwrap()
}

pub struct EmbeddingModelFilePaths<'a> {
    pub onnx: &'a str,
    pub tokenizer: &'a str,
    pub config: &'a str,
    pub special_tokens_map: &'a str,
    pub tokenizer_config: &'a str,
}

impl EmbeddingModelFilePaths<'_> {
    pub fn to_user_defined_embedding_model(self) -> Result<UserDefinedEmbeddingModel, io::Error> {
        let onnx_file = read(self.onnx)?;
        let tokenizer_file = read(self.tokenizer)?;
        let config_file = read(self.config)?;
        let special_tokens_map_file = read(self.special_tokens_map)?;
        let tokenizer_config_file = read(self.tokenizer_config)?;

        Ok(UserDefinedEmbeddingModel::new(
            onnx_file,
            TokenizerFiles {
                tokenizer_file,
                config_file,
                special_tokens_map_file,
                tokenizer_config_file,
            },
        ))
    }
}
