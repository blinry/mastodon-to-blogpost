use megalodon::entities::status::Status;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let m = MastodonToBlogpost::new("https://chaos.social");
    let start_toot = "https://chaos.social/@blinry/112990469485308286";
    let blogpost = m.convert_thread(start_toot).await?;
    println!("{}", blogpost.markdown);
    println!("{:?}", blogpost.files);
    Ok(())
}

#[derive(Debug)]
struct Blogpost {
    markdown: String,
    files: Vec<String>,
}

struct MastodonToBlogpost {
    client: Box<dyn megalodon::Megalodon>,
}

impl MastodonToBlogpost {
    fn new(domain: &str) -> Self {
        let access_token = std::env::var("ACCESS_TOKEN").ok();
        let client = megalodon::generator(
            megalodon::SNS::Mastodon,
            String::from(domain),
            access_token,
            None,
        );
        Self { client }
    }

    async fn convert_thread(&self, toot: &str) -> anyhow::Result<Blogpost> {
        let mut metadata = HashMap::new();

        let id = toot.split('/').last().unwrap().to_string();
        let status = self.client.get_status(id.clone()).await?.json();

        // Add the metadata we have.
        metadata.insert(
            "published".to_string(),
            format!("{}", &status.created_at.format("%Y-%m-%d")),
        );
        metadata.insert("toot".to_string(), toot.to_string());
        let mut markdown = "---\n".to_string();
        for (key, value) in metadata {
            markdown += &format!("{}: {}\n", key, value);
        }
        markdown += "---\n\n";

        let first_toot = self.convert(&status).await?;
        markdown += &first_toot.markdown;
        let mut files = first_toot.files;

        let account = status.account;

        let context = self.client.get_status_context(id, None).await?.json();

        for status in context.descendants {
            // Skip replies from other users.
            if status.account != account {
                continue;
            }

            // Skip replies to other people.
            if let Some(ref reply_to_account_id) = status.in_reply_to_account_id {
                if reply_to_account_id != &account.id {
                    continue;
                }
            }

            markdown += "\n\n";
            let toot = self.convert(&status).await?;
            markdown += &toot.markdown;
            files.extend(toot.files);
        }

        // Collapse more than one empty line.
        let re = regex::Regex::new(r"\n\n\n+").unwrap();
        markdown = re.replace_all(&markdown, "\n\n").to_string();

        Ok(Blogpost { markdown, files })
    }

    async fn convert(&self, status: &Status) -> anyhow::Result<Blogpost> {
        let mut markdown = mdka::from_html(&status.content);

        // Fix spaces after @ characters.
        markdown = markdown.replace("[@ ", "[@");

        let mut files = vec![];
        for attachment in &status.media_attachments {
            let alt_text = if let Some(ref description) = attachment.description {
                format!(" \"{}\"", description)
            } else {
                "".to_string()
            };
            markdown += &format!(
                "\n\n![{0}]({1}{2})",
                attachment.description.clone().unwrap_or("".to_string()),
                attachment.url.split('/').last().unwrap(),
                alt_text,
            );
            files.push(attachment.url.clone());
        }

        Ok(Blogpost { markdown, files })
    }
}
