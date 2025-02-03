use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BuildStreamError, SampleRate, SizedSample,
};
use rustpotter::{Rustpotter, RustpotterConfig, Sample, SampleFormat, ScoreMode};
use std::sync::mpsc;
use thiserror::Error;

/// WakewordConfig can be used to configure the wakeword listener. It can be created by calling
/// [WakewordConfig::build]. Wakewords can be added by calling [WakewordConfig::add_wakeword_from_file] and the
/// listener can be started by calling [WakewordConfig::start].
pub struct WakewordConfig {
    rustpotter: Rustpotter,
    input_device: cpal::Device,
    input_config: cpal::SupportedStreamConfig,
    stream_config: cpal::StreamConfig,
    wakeword_added: bool,
}

#[derive(Error, Debug)]
pub enum WakewordConfigBuildError {
    #[error("No input device available")]
    NoInputDevice,
    #[error("No default input config available")]
    NoDefaultInputConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("Failed to list input configs")]
    ListInputConfigs(#[from] cpal::SupportedStreamConfigsError),
    #[error("Failed to get a supported input config")]
    GetSupportedInputConfig,
    #[error("Wrong sample format size")]
    WrongSampleFormatSize,
    #[error("Failed to create Rustpotter")]
    CreateRustpotter(String),
}

#[derive(Error, Debug)]
pub enum WakewordConfigStartError {
    #[error("Failed to init input stream")]
    InitInputStream(#[from] BuildStreamError),
    #[error("Failed to play stream")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("No wakewords added")]
    NoWakewordsAdded,
}

#[derive(Error, Debug)]
#[error("Failed to add wakeword: {0}")]
pub struct WakewordConfigAddError(String);

impl WakewordConfig {
    /// Create a new WakewordConfig. This function will try to find a compatible input device and
    /// configuration. If no compatible configuration is found, it will return an error.
    pub fn build() -> Result<Self, WakewordConfigBuildError> {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .ok_or(WakewordConfigBuildError::NoInputDevice)?;

        let default_input_config = input_device.default_input_config()?;

        let input_config = if is_compatible_format(&default_input_config.sample_format()) {
            default_input_config
        } else {
            // look for any compatible configuration
            input_device
                .supported_input_configs()?
                .find(|sc| is_compatible_format(&sc.sample_format()))
                .map(|sc| try_get_config_with_sample_rate(sc, 16000))
                .ok_or(WakewordConfigBuildError::GetSupportedInputConfig)?
        };

        let stream_config = cpal::StreamConfig {
            channels: input_config.channels(),
            sample_rate: input_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let bits_per_sample = (input_config.sample_format().sample_size() * 8) as u16;
        let mut config = RustpotterConfig::default();

        config.fmt.sample_rate = input_config.sample_rate().0 as usize;
        config.fmt.channels = input_config.channels();
        config.fmt.sample_format = if input_config.sample_format().is_float() {
            SampleFormat::float_of_size(bits_per_sample)
        } else {
            SampleFormat::int_of_size(bits_per_sample)
        }
        .ok_or(WakewordConfigBuildError::WrongSampleFormatSize)?;

        // Defaults from rustpotter-cli
        config.detector.avg_threshold = 0.;
        config.detector.threshold = 0.5;
        config.detector.min_scores = 10;
        config.detector.eager = true;
        config.detector.score_mode = ScoreMode::Max;
        config.detector.score_ref = 0.22;
        config.detector.vad_mode = None;
        // config.detector.record_path = None; // Requires `record` feature
        config.filters.gain_normalizer.enabled = false;
        config.filters.gain_normalizer.gain_ref = None;
        config.filters.gain_normalizer.min_gain = 0.1;
        config.filters.gain_normalizer.max_gain = 1.;
        config.filters.band_pass.enabled = false;
        config.filters.band_pass.low_cutoff = 80.;
        config.filters.band_pass.high_cutoff = 400.;

        let rustpotter =
            Rustpotter::new(&config).map_err(WakewordConfigBuildError::CreateRustpotter)?;

        Ok(WakewordConfig {
            rustpotter,
            input_device,
            input_config,
            stream_config,
            wakeword_added: false,
        })
    }

    /// Add a wakeword from a file. The file should be in the Rustpotter Wakeword format.
    /// The name is used to identify the wakeword when it is detected.
    /// This function will return an error if the file could not be read or if the wakeword could
    /// not be added.
    pub fn add_wakeword_from_file(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<(), WakewordConfigAddError> {
        self.rustpotter
            .add_wakeword_from_file(name, path)
            .map_err(WakewordConfigAddError)?;
        self.wakeword_added = true;
        Ok(())
    }

    /// Start listening for wakewords. This function will return a WakewordListener that can be
    /// used to listen for wakewords.
    pub fn start(self) -> Result<WakewordListener, WakewordConfigStartError> {
        if !self.wakeword_added {
            return Err(WakewordConfigStartError::NoWakewordsAdded);
        }

        let (tx, rx) = mpsc::channel();

        let stream = match self.input_config.sample_format() {
            cpal::SampleFormat::I16 => init_input_stream(
                &self.input_device,
                self.stream_config,
                self.rustpotter,
                Vec::<i16>::new(),
                tx,
            )?,
            cpal::SampleFormat::I32 => init_input_stream(
                &self.input_device,
                self.stream_config,
                self.rustpotter,
                Vec::<i32>::new(),
                tx,
            )?,
            cpal::SampleFormat::F32 => init_input_stream(
                &self.input_device,
                self.stream_config,
                self.rustpotter,
                Vec::<f32>::new(),
                tx,
            )?,
            _ => panic!("The only supported sample formats are i16, i32 and f32. This should never happen, because we already checked for this in WakewordConfig::build."),
        };

        stream.play()?;

        Ok(WakewordListener { rx, stream })
    }
}

/// WakewordListener can be used to listen for wakewords and can only be created by
/// calling [WakewordConfig::start].
pub struct WakewordListener {
    rx: mpsc::Receiver<String>,
    #[allow(dead_code)]
    stream: cpal::Stream,
}

impl WakewordListener {
    /// Listen for wakewords. This function will block until a wakeword is detected.
    pub fn listen(&self) -> Result<String, mpsc::RecvError> {
        self.rx.recv()
    }

    /// Returns an iterator over detected wakewords.
    pub fn listen_iter(&self) -> mpsc::Iter<String> {
        self.rx.iter()
    }
}

fn is_compatible_format(format: &cpal::SampleFormat) -> bool {
    matches!(
        format,
        cpal::SampleFormat::I16 | cpal::SampleFormat::I32 | cpal::SampleFormat::F32
    )
}

fn try_get_config_with_sample_rate(
    sc: cpal::SupportedStreamConfigRange,
    preferred_sample_rate: u32,
) -> cpal::SupportedStreamConfig {
    if sc.min_sample_rate().0 <= preferred_sample_rate
        && preferred_sample_rate <= sc.max_sample_rate().0
    {
        sc.with_sample_rate(SampleRate(preferred_sample_rate))
    } else {
        sc.with_max_sample_rate()
    }
}

fn init_input_stream<S: Sample + SizedSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut rustpotter: Rustpotter,
    mut buffer: Vec<S>,
    mut tx: mpsc::Sender<String>,
) -> Result<cpal::Stream, BuildStreamError> {
    let error_callback = move |err| {
        eprintln!("an error occurred on stream: {}", err);
    };

    let rustpotter_samples_per_frame = rustpotter.get_samples_per_frame();
    let data_callback = move |data: &[S], _: &_| {
        run_detection(
            &mut rustpotter,
            data,
            &mut buffer,
            rustpotter_samples_per_frame,
            &mut tx,
        )
    };
    device.build_input_stream(&config, data_callback, error_callback, None)
}

fn run_detection<T: Sample>(
    rustpotter: &mut Rustpotter,
    data: &[T],
    buffer: &mut Vec<T>,
    rustpotter_samples_per_frame: usize,
    tx: &mut mpsc::Sender<String>,
) {
    buffer.extend_from_slice(data);
    while buffer.len() >= rustpotter_samples_per_frame {
        let detection = rustpotter.process_samples(
            buffer
                .drain(0..rustpotter_samples_per_frame)
                .as_slice()
                .into(),
        );
        if let Some(detection) = detection {
            // println!("Wakeword detection: {:?}", detection);
            tx.send(detection.name).unwrap();
        }
    }
}
