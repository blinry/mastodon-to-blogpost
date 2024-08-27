use megalodon::entities::status::Status;
use std::path::PathBuf;
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let start_toot = Url::parse(&std::env::args().nth(1).expect("Please provide a toot URL."))?;
    let m = MastodonToBlogpost::new(&start_toot.origin().ascii_serialization());

    let blogpost = m.convert_thread(&start_toot).await?;
    println!("{}", blogpost.markdown);

    // Write to index.md.
    std::fs::write("index.md", blogpost.markdown)?;

    // Download the files, if they don't exist yet.
    for (url, filename) in blogpost.files {
        if !filename.exists() {
            println!("Downloading {:?}...", filename);
            let response = reqwest::get(url.as_str()).await?;
            let bytes = response.bytes().await?;
            std::fs::write(&filename, bytes)?;
        }
    }

    Ok(())
}

#[derive(Debug)]
struct Blogpost {
    markdown: String,
    // URL and target filename.
    files: Vec<(Url, PathBuf)>,
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

    async fn convert_thread(&self, toot: &Url) -> anyhow::Result<Blogpost> {
        let mut markdown = String::new();

        let id = toot.path_segments().unwrap().last().unwrap().to_string();
        let status = self.client.get_status(id.clone()).await?.json();

        // Add the metadata we have.
        markdown += "---\n";
        markdown += "title: \n";
        markdown += "tags: \n";
        markdown += "thumbnail: \n";
        markdown += format!("published: {}\n", status.created_at.format("%Y-%m-%d")).as_str();
        markdown += format!("toot: {}\n", toot).as_str();
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

        // Remove empty lines at the end of the file.
        markdown = markdown.trim_end().to_string() + "\n";

        Ok(Blogpost { markdown, files })
    }

    async fn convert(&self, status: &Status) -> anyhow::Result<Blogpost> {
        let mut markdown = mdka::from_html(&status.content);

        // Fix spaces after @ characters.
        markdown = markdown.replace("[@ ", "[@");

        let mut files = vec![];
        for attachment in &status.media_attachments {
            let filename = filename_for(&Url::parse(&attachment.url)?);

            let alt_text = if let Some(ref description) = attachment.description {
                format!(" \"{}\"", description)
            } else {
                "".to_string()
            };
            markdown += &format!(
                "\n\n![{0}]({1}{2})",
                attachment.description.clone().unwrap_or("".to_string()),
                filename.to_str().unwrap(),
                alt_text,
            );

            files.push((Url::parse(&attachment.url)?, filename));
        }

        Ok(Blogpost { markdown, files })
    }
}

fn filename_for(url: &Url) -> PathBuf {
    let mut filename = PathBuf::from(url.path_segments().unwrap().last().unwrap().to_string());
    let extension = filename.extension().unwrap().to_str().unwrap();
    if extension == "jpeg" {
        filename.set_extension("jpg");
    }
    filename
}
