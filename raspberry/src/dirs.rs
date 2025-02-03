use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn get_config_path() -> PathBuf {
    let home_dir = env::var("HOME").expect("Failed to get HOME directory");
    let config_dir = PathBuf::from(home_dir).join(".config/raspberry");
    fs::create_dir_all(&config_dir).expect("Failed to create config directory");
    config_dir
}

pub fn get_config_file<P: AsRef<Path>>(config: &Path, file: P) -> PathBuf {
    config.join(file)
}
