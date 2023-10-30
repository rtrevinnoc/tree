use anyhow::Result;
use finalfusion::{compat::text::ReadText, embeddings::Embeddings};
use futures::StreamExt;
use lingua::Language::{English, Spanish};
use lingua::{LanguageDetector, LanguageDetectorBuilder};
use reqwest::Url;
use rocket::serde::json;
use std::collections::{HashMap, HashSet};
use std::env::var;
use std::{fs::File, io::BufReader};
use tree::{get_sentence_embedding, CrawledEntry};
use uuid::Uuid;
use voyager::{
    scraper::Selector,
    {Collector, Crawler, CrawlerConfig, Response, Scraper},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    struct Explorer {
        /// visited urls mapped with all the urls that link to that url
        visited: HashMap<Url, HashSet<Url>>,
        link_selector: Selector,
        title_selector: Selector,
        header_selector: Selector,
        meta_title_selector: Selector,
        meta_site_name_selector: Selector,
        meta_description_selector: Selector,
    }
    impl Default for Explorer {
        fn default() -> Self {
            Self {
                visited: Default::default(),
                link_selector: Selector::parse("a").unwrap(),
                title_selector: Selector::parse("title").unwrap(),
                header_selector: Selector::parse("h1").unwrap(),
                meta_title_selector: Selector::parse("meta[property=\"title\"], meta[property=\"og:title\"]").unwrap(),
                meta_site_name_selector: Selector::parse("meta[property=\"site_name\"], meta[property=\"og:site_name\"]").unwrap(),
                meta_description_selector: Selector::parse("meta[property=\"description\"], meta[name=\"description\"], meta[property=\"og:description\"]").unwrap(),
            }
        }
    }

    impl Scraper for Explorer {
        type Output = (Url, String, String, String, usize);
        type State = Url;

        fn scrape(
            &mut self,
            mut response: Response<Self::State>,
            crawler: &mut Crawler<Self>,
        ) -> Result<Option<Self::Output>> {
            if let Some(origin) = response.state.take() {
                self.visited
                    .entry(response.response_url.clone())
                    .or_default()
                    .insert(origin);
            }

            for link in response.html().select(&self.link_selector) {
                if let Some(href) = link.value().attr("href") {
                    if let Ok(url) = response.response_url.join(href) {
                        crawler.visit_with_state(url, response.response_url.clone());
                    }
                }
            }

            let mut title = String::from("");
            match response.html().select(&self.meta_site_name_selector).next() {
                Some(value) => {
                    if let Some(value) = value.value().attr("content") {
                        title = value.trim().to_owned();
                    }
                }
                None => {
                    if let Some(value) = response.html().select(&self.title_selector).next() {
                        if let Some(value) = value.text().next() {
                            title = value.trim().to_owned();
                        }
                    }
                }
            }

            let mut header = String::from("");
            match response.html().select(&self.meta_title_selector).next() {
                Some(value) => {
                    if let Some(value) = value.value().attr("content") {
                        header = value.trim().to_owned();
                    }
                }
                None => {
                    if let Some(value) = response.html().select(&self.header_selector).next() {
                        if let Some(value) = value.text().next() {
                            header = value.trim().to_owned();
                        }
                    }
                }
            }

            let mut description = String::from("");
            if let Some(value) = response
                .html()
                .select(&self.meta_description_selector)
                .next()
            {
                if let Some(value) = value.value().attr("content") {
                    description = value.trim().to_owned();
                }
            }

            Ok(Some((
                response.response_url,
                title,
                header,
                description,
                response.depth,
            )))
        }
    }

    let max_concurrent_requests = match var("MAX_CONCURRENT_REQUESTS") {
        Ok(number) => number.parse().unwrap_or(100),
        Err(_) => 100,
    };

    let config = CrawlerConfig::default()
        .disallow_domains(vec!["facebook.com", "google.com"])
        // stop after 3 jumps
        .max_depth(4)
        // maximum of requests that are active
        .max_concurrent_requests(max_concurrent_requests);
    // .respect_robots_txt();

    let mut collector = Collector::new(Explorer::default(), config);

    match var("START_URL") {
        Ok(url) => {
            collector.crawler_mut().visit(url);
        }
        Err(e) => {
            println!("Error: {:?}. Set the START_URL environment variable to where you want to start crawling.", e);
            return Ok(());
        }
    }

    let mut p = project_root::get_project_root().unwrap();
    p.push("glove.6B/glove.6B.50d.txt");
    let mut reader = BufReader::new(File::open(p).unwrap());

    let embeddings = Embeddings::read_text(&mut reader).unwrap();
    let db = sled::open("urlDatabase").expect("open");

    let languages = vec![English, Spanish];
    let detector: LanguageDetector = LanguageDetectorBuilder::from_languages(&languages).build();

    while let Some(output) = collector.next().await {
        if let Ok((url, title, header, description, _)) = output {
            let url_string: String = url.clone().into();
            let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, url_string.as_bytes());
            let language = match detector.detect_language_of(&title) {
                Some(language) => language.iso_code_639_1().to_string(),
                None => String::from("unk"),
            };
            if let Some(vec) = get_sentence_embedding(&embeddings, &title).await {
                let crawled_json = CrawledEntry {
                    url: url_string,
                    title: title.clone(),
                    header,
                    description,
                    vec: vec.to_vec(),
                    language,
                };

                if let Ok(_) = db.insert(
                    uuid.as_u128().to_string(),
                    json::to_string(&crawled_json).unwrap().as_str(),
                ) {
                    print!("Crawled {}\n", url);
                }
            }
        }
    }

    Ok(())
}
