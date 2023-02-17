use anyhow::Result;
use futures::StreamExt;
use reqwest::Url;
use rocket::serde::json;
use std::collections::{HashMap, HashSet};
use tree::{get_sentence_embedding, CrawledEntry};
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

            let title;
            match response.html().select(&self.meta_site_name_selector).next() {
                Some(value) => {
                    title = value.value().attr("content").unwrap().trim().to_string();
                }
                None => match response.html().select(&self.title_selector).next() {
                    Some(value) => {
                        title = value.text().next().unwrap().trim().to_string();
                    }
                    None => title = "".to_string(),
                },
            }

            let header;
            match response.html().select(&self.meta_title_selector).next() {
                Some(value) => {
                    header = value.value().attr("content").unwrap().trim().to_string();
                }
                None => match response.html().select(&self.header_selector).next() {
                    Some(value) => {
                        header = value.text().next().unwrap().trim().to_string();
                    }
                    None => header = "".to_string(),
                },
            }

            let description;
            match response
                .html()
                .select(&self.meta_description_selector)
                .next()
            {
                //Some(value) => { header = value.text().flat_map(|s| s.trim().chars()).collect::<String>(); }
                Some(value) => {
                    description = value.value().attr("content").unwrap().trim().to_string();
                }
                None => description = "".to_string(),
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

    let config = CrawlerConfig::default()
        .disallow_domains(vec!["facebook.com", "google.com"])
        // stop after 3 jumps
        .max_depth(4)
        // maximum of requests that are active
        .max_concurrent_requests(1_000);
    let mut collector = Collector::new(Explorer::default(), config);

    collector
        .crawler_mut()
        .visit("https://jakedawkins.com/2020-04-16-unwrap-expect-rust/"); //.visit("https://www.wikipedia.org/");

    let db = sled::open("urlDatabase").expect("open");

    let mut index = 0;
    if let Ok(last_result) = db.last() {
        if let Some(last_option) = last_result {
            index = String::from_utf8_lossy(&last_option.0).parse().unwrap();
        }
    }

    while let Some(output) = collector.next().await {
        if let Ok((url, title, header, description, _)) = output {
            if let Some(vec) = get_sentence_embedding(&title) {
                let crawled_json = CrawledEntry {
                    url: url.into(),
                    title,
                    header,
                    description,
                    vec: vec.to_vec(),
                };

                if let Ok(_) = db.insert(
                    index.to_string().as_str(),
                    json::to_string(&crawled_json).unwrap().as_str(),
                ) {
                    // println!("Crawled {}\n", json::to_string(&crawled_json).unwrap());
                    index = index + 1;
                }
            }
        }
    }

    Ok(())
}
