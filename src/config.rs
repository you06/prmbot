use std::{fs::read_to_string, io::Error, time::Duration};

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    #[serde(rename = "slack-token")]
    pub slack_token: String,
    #[serde(rename = "slack-channel")]
    pub slack_channel: String,

    #[serde(rename = "github-token")]
    pub github_token: String,
    #[serde(default)]
    #[serde(rename = "repos")]
    pub repos: Vec<String>,
    #[serde(default)]
    #[serde(rename = "deliver-labels")]
    pub deliver_labels: Vec<String>,
    #[serde(with = "humantime_serde")]
    #[serde(rename = "deliver-after")]
    pub deliver_after: Duration,
}

impl Config {
    pub fn new(filename: String) -> Result<Self, Error> {
        let contents = read_to_string(filename)?;
        let config: Config = toml::from_str(&contents[..]).unwrap();
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_config() -> Result<Config, Error> {
        Config::new("config.example.toml".to_owned())
    }

    #[test]
    fn read_config() {
        let config = new_config().unwrap();
        // slack
        assert_eq!(config.slack_token, "slack-token");
        assert_eq!(config.slack_channel, "slack-channel");
        // github
        assert_eq!(config.github_token, "github-token");
        assert_eq!(config.repos, vec!("you06/prmbot"));
        assert_eq!(config.deliver_labels, vec!["type/feature-request"]);
        assert_eq!(config.deliver_after, Duration::new(12 * 60 * 60, 0));
    }
}
