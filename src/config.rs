use failure::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use toml;
use xdg::BaseDirectories;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub font: String,
    pub font_bold: String,
}
impl Default for Config {
    fn default() -> Self {
        Config {
            font: String::new(),
            font_bold: String::new(),
        }
    }
}
impl Config {
    pub fn load() -> Result<Self, Error> {
        let xdg = BaseDirectories::with_prefix("orbterm")?;
        if let Some(path) = xdg.find_config_file("config") {
            Config::read(&path)
        } else {
            let path = xdg.place_config_file("config")?;
            let config = Config::default();
            config.write(&path)?;
            Ok(config)
        }
    }

    pub fn read<P: AsRef<Path>>(path: &P) -> Result<Self, Error> {
        let mut file = File::open(path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        toml::from_slice(&contents).map_err(Error::from)
    }

    pub fn write<P: AsRef<Path>>(&self, path: &P) -> Result<(), Error> {
        let contents = toml::to_string_pretty(&self)?;
        let mut file = File::create(path)?;
        file.write_all(contents.as_bytes()).map_err(Error::from)
    }
}
