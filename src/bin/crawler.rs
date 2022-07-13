use anyhow::Result;
use futures::StreamExt;
use reqwest::Url;
use std::collections::{
    HashMap,
    HashSet
};
use voyager::{
    scraper::Selector,
    {
        Collector,
        Crawler,
        CrawlerConfig,
        Response,
        Scraper
    }
};
use rod::{
    Node,
    Config,
    Value
};
use tree::{
    get_word_embedding,
    get_sentence_embedding
};
use rocket::serde::{
    Serialize,
    Deserialize,
    json
};
use uuid::Uuid;

#[derive(Serialize,Deserialize)]
struct CrawledEntry {
    url: String,
    title: String,
    header: String,
    description: String,
}

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
                None => {

                    match response.html().select(&self.title_selector).next() {
                        Some(value) => {
                            title = value.text().next().unwrap().trim().to_string();
                        }
                        None => {
                            title = "".to_string()
                        }
                    }

                }
            }

            let header;
            match response.html().select(&self.meta_title_selector).next() {
                Some(value) => {

                    header = value.value().attr("content").unwrap().trim().to_string();

                }
                None => {

                    match response.html().select(&self.header_selector).next() {
                        Some(value) => {
                            header = value.text().next().unwrap().trim().to_string();
                        }
                        None => {
                            header = "".to_string()
                        }
                    }

                }
            }

            let description;
            match response.html().select(&self.meta_description_selector).next() {
                //Some(value) => { header = value.text().flat_map(|s| s.trim().chars()).collect::<String>(); }
                Some(value) => {

                    description = value.value().attr("content").unwrap().trim().to_string();

                }
                None => {

                    description = "".to_string()

                }
            }

            Ok(Some((response.response_url, title, header, description, response.depth)))
        }
    }

    let config = CrawlerConfig::default()
        .disallow_domains(vec!["facebook.com", "google.com"])
        // stop after 3 jumps
        .max_depth(4)
        // maximum of requests that are active
        .max_concurrent_requests(1_000);
    let mut collector = Collector::new(Explorer::default(), config);

    collector.crawler_mut().visit("https://docs.rs/scraper/0.12.0/scraper/selector/struct.Selector.html#method.parse");//.visit("https://www.wikipedia.org/");

    let mut db = Node::new_with_config(Config {
        outgoing_websocket_peers: vec!["wss://rtrevc.uber.space/ws".to_string()],
        ..Config::default()
    });

    //let mut sub = db.get("greeting").on();
    ////db.get("greeting").put("Hello World!".into());
    //if let Value::Text(str) = sub.recv().await.unwrap() {
        //println!("{}", &str);
        //assert_eq!(&str, "Hello World!");
    //}
    
    //let mut reader = BufReader::new(File::open("./glove.6B/glove.6B.50d.txt").unwrap());
    //let embeddings = Embeddings::read_text(&mut reader).unwrap();
    //let embedding = embeddings.embedding("future");
    //dbg!(embedding);

    while let Some(output) = collector.next().await {
        if let Ok((url, title, header, description, _)) = output {
            let url_string: String = url.into();
            let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, url_string.as_bytes());
            let crawled_json = CrawledEntry {
                url: url_string,
                title,
                header,
                description
            };
            println!("Crawled ({:?}): {}\n", uuid.as_bytes(), json::to_string(&crawled_json).unwrap());
        }
    }

    Ok(())
}
