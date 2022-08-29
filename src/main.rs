use json::object;
use log::debug;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

#[derive(Debug)]
struct Config {
    webhook_url: Url,
    fetch_urls: Vec<Url>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RawConfig {
    webhook_url: String,
    fetch_urls: Vec<String>,
}

impl RawConfig {
    fn parse(&self) -> Result<Config, Box<dyn std::error::Error>> {
        Ok(Config {
            webhook_url: Url::parse(self.webhook_url.as_str())?,
            fetch_urls: self
                .fetch_urls
                .iter()
                .map(|url| Url::parse(url))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = serde_json::from_str::<RawConfig>(env::var("CONFIG")?.as_str())?.parse()?;
    dbg!(&config);

    for url in config.fetch_urls.iter() {
        let text = reqwest::get(url.to_string()).await?.text().await?;
        let kakomon;
        match extract_kakomon(&text, url.clone()) {
            Some(kako) => kakomon = kako,
            _ => continue,
        }

        let body = object! {
            "blocks": [
                {
                    "type": "header",
                    "text": {
                        "type": "plain_text",
                        "text": kakomon.title.clone(),
                        "emoji": true
                    }
                },
                {
                    "type": "divider",
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": kakomon.text.clone(),
                    }
                }
            ]
        };

        // send to webhook urls.
        send_to_slack_webhook(&config.webhook_url, body.to_string()).await?;
    }

    Ok(())
}

async fn send_to_slack_webhook(
    webhook: &Url,
    body: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let res = client
        .post(webhook.to_string())
        .header("Content-type", "application/json")
        .body(body)
        .send()
        .await?;
    debug!("{:?}", res.status());

    Ok(())
}

#[derive(Debug, Clone)]
struct Kakomon {
    title: String,
    text: String,
}

// get the first div element having "kako" class
fn extract_kakomon(html_text: &str, url: Url) -> Option<Kakomon> {
    let document = Html::parse_document(html_text);
    for element in document.select(&Selector::parse(r#"div"#).unwrap()) {
        let mut title = String::new();
        let mut text = String::new();

        if element.value().attr("class") == Some("kako") {
            // get the url to the answer page
            for elem2 in element.select(&Selector::parse(r#"a"#).unwrap()) {
                let href = elem2.value().attr("href").unwrap();
                match Url::parse(href) {
                    Ok(url) => {
                        text += url.to_string().as_str();
                    }
                    Err(_) => {
                        text += url.join(href).unwrap().to_string().as_str();
                    }
                }
                text += "\n";
            }

            // get the problem statement
            for elem2 in element.select(&Selector::parse(r#"div"#).unwrap()) {
                match elem2.value().attr("class") {
                    Some("mondai") => {
                        text += elem2.text().collect::<Vec<_>>().join("").as_str();
                        text += "\n";
                    }
                    Some("anslink") => {
                        title += elem2.text().collect::<Vec<_>>().join("").as_str();
                    }
                    Some("ansbg") => {
                        // answer background

                        for (elem3_idx, elem3) in elem2
                            .select(&Selector::parse(r#"ul > li"#).unwrap())
                            .enumerate()
                        {
                            text += format!("{}. ", elem3_idx + 1).as_str();
                            text += elem3.text().collect::<Vec<_>>().join("").as_str();
                            text += "\n";
                        }
                    }
                    _ => {}
                }
            }

            // get urls of images
            for elem2 in element.select(&Selector::parse(r#"img"#).unwrap()) {
                let href = elem2.value().attr("src").unwrap();
                match Url::parse(href) {
                    Ok(url) => {
                        text += url.to_string().as_str();
                    }
                    Err(_) => {
                        text += url.join(href).unwrap().to_string().as_str();
                    }
                }
                text += "\n";
            }
            return Some(Kakomon {
                title: title,
                text: text,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_kakomon_url_from_home() {
        let html_text = include_str!("../testdata/home.html");
        let url = Url::parse("https://www.ap-siken.com/").expect("invalid url");
        let kakomon = extract_kakomon(html_text, url).unwrap();

        // trim
        let left = kakomon
            .text
            .split("\n")
            .map(|s| s.trim())
            .collect::<Vec<_>>()
            .join("\n");
        let right = r#"https://www.ap-siken.com/kakomon/21_haru/q31.html
クライアントサーバシステムにおけるストアドプロシージャに関する記述のうち，誤っているものはどれか。
1. 機密性の高いデータに対する処理を特定のプロシージャ呼出しに限定することによって，セキュリティを向上させることができる。
2. システム全体に共通な処理をプロシージャとして格納することによって，処理の標準化を行うことができる。
3. データベースへのアクセスを細かい単位でプロシージャ化することによって，処理性能(スループット)を向上させることができる。
4. 複数のSQL文から成る手続を1回の呼出しで実行できるので，クライアントとサーバ間の通信回数を減らすことができる。
"#;
        println!("{}", &left);
        println!("{}", &right);
        assert_eq!(left.as_str(), right);
    }
}
