#[macro_use]
extern crate rocket;
use finalfusion::{
    compat::text::ReadText, embeddings::Embeddings, storage::NdArray, vocab::SimpleVocab,
};
use hora::core::ann_index::ANNIndex;
use rocket::serde::{json, json::Json, Serialize};
use rocket::State;
use std::{fs::File, io::BufReader};
use tree::{CrawledEntry, Embedding};
mod dbpedia;

#[derive(Serialize)]
struct Url {
    url: String,
    title: String,
    header: String,
    description: String,
}

#[derive(Serialize)]
struct Answer {
    answer: String,
    urls: Vec<Url>,
    small_summary: String,
    corrected: String,
}

struct Config {
    vec_index: hora::index::hnsw_idx::HNSWIndex<f32, usize>,
    db: sled::Db,
    embeddings: Embeddings<SimpleVocab, NdArray>,
}

#[get("/?<query>&<page>")]
async fn _answer(state: &State<Config>, query: &str, page: usize) -> Json<Answer> {
    let page_size = 5;

    let mut urls: Vec<Url> = Vec::new();
    if let Some(query_vec) = state.embeddings.get_sentence_embedding(query) {
        for vec in state
            .vec_index
            .search(&query_vec.to_vec(), page_size * page)
            .split_off(page_size * (page - 1))
        {
            if let Ok(value_result) = state.db.get(vec.to_string().as_str()) {
                if let Some(value_option) = value_result {
                    match json::from_str::<CrawledEntry>(
                        String::from_utf8_lossy(&value_option).as_ref(),
                    ) {
                        Ok(url_value) => {
                            urls.push(Url {
                                url: url_value.url,
                                title: url_value.title,
                                header: url_value.header,
                                description: url_value.description,
                            });
                        }
                        Err(_) => {}
                    }
                }
            }
        }
    }

    let dbpedia_resource = dbpedia::get_resource(query).await.unwrap();
    let answer = dbpedia::get_summary(&dbpedia_resource).await.unwrap();

    Json(Answer {
        urls,
        small_summary: answer.clone(),
        answer,
        corrected: query.to_string(),
    })
}

#[launch]
fn rocket() -> _ {
    let mut p = project_root::get_project_root().unwrap();
    p.push("glove/glove.6B.50d.txt");
    dbg!(p);
    let mut reader = BufReader::new(File::open("glove.6B/glove.6B.50d.txt").unwrap());

    let embeddings = Embeddings::read_text(&mut reader).unwrap();
    let db = sled::open("urlDatabase").expect("open");
    let mut index = hora::index::hnsw_idx::HNSWIndex::<f32, usize>::new(
        50,
        &hora::index::hnsw_params::HNSWParams::<f32>::default(),
    );

    for url in db.iter() {
        if let Ok(url) = url {
            let url_key: usize = String::from_utf8_lossy(&url.0).parse().unwrap();
            match json::from_str::<CrawledEntry>(String::from_utf8_lossy(&url.1).as_ref()) {
                Ok(url_value) => {
                    index.add(&url_value.vec, url_key).unwrap();
                }
                Err(_) => {}
            }
        }
    }

    index
        .build(hora::core::metrics::Metric::CosineSimilarity)
        .unwrap();

    let index = index;

    let config = Config {
        vec_index: index,
        db,
        embeddings,
    };

    rocket::build()
        .manage(config)
        .mount("/_answer", routes![_answer])
}
