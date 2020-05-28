mod config;
mod providers;

use clap::Clap;
use config::Config;
use providers::github::GitHub;
use providers::slack::Slack;

#[derive(Clap)]
#[clap(version = "1.0", author = "you06")]
struct Opts {
    #[clap(short = "c", long = "config", default_value = "config.toml")]
    config: String,
    #[clap(short = "p", long = "ping")]
    ping: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts: Opts = Opts::parse();
    let conf = Config::new(opts.config).unwrap();

    if let Some(ping) = opts.ping {
        let slack_client = Slack::new(conf.slack_token.clone());
        let _ = slack_client
            .send_message(conf.slack_channel.clone(), ping)
            .await?;
        return Ok(());
    }

    let mut report = "".to_owned();
    let mut has_issue = false;

    let github_client = GitHub::new(
        conf.github_token.to_owned(),
        conf.deliver_labels.clone(),
        conf.deliver_after.clone(),
    );
    let user = github_client.get_user_result().await;
    println!("Current user: {}", user.unwrap());

    let issues = github_client.get_opened_issues(conf.repos.clone()).await?;

    if issues.len() != 0 {
        has_issue = true;
        report.push_str(&format!("{} no-reply issues\n", issues.len())[..]);
        for issue in issues {
            report.push_str(&format!("{}\n", issue)[..]);
        }
    }

    if conf.slack_token != "" && conf.slack_channel != "" {
        if has_issue {
            let slack_client = Slack::new(conf.slack_token.clone());
            let _ = slack_client
                .send_message(conf.slack_channel.clone(), report)
                .await?;
        }
    } else {
        println!("{}", report);
    }
    Ok(())
}
