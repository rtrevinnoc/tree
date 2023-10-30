#[macro_use]
extern crate rocket;
use finalfusion::{compat::text::ReadText, embeddings::Embeddings};
use hora::core::ann_index::ANNIndex;
use rocket::http::Status;
use rocket::serde::{json, json::Json, Deserialize, Serialize};
use rocket::State;
use std::{fs::File, io::BufReader};
use tree::{get_url_list, Config, CrawledEntry, Url};
mod dbpedia;

#[derive(Serialize)]
struct Answer {
    answer: String,
    urls: Vec<Url>,
    small_summary: String,
    corrected: String,
}

#[derive(Serialize)]
struct Results {
    urls: Vec<Url>,
}

#[derive(Serialize)]
struct Peers {
    peers: Vec<Peer>,
}

#[derive(Serialize, Deserialize)]
struct Peer {
    address: String,
}

#[get("/?<query>&<page>&<language_option>")]
async fn _answer(
    state: &State<Config>,
    query: &str,
    page: usize,
    language_option: Option<&str>,
) -> Result<Json<Answer>, Status> {
    let page_size = 5;

    let urls = match get_url_list(state, query, page, page_size, language_option).await {
        Ok(results) => results,
        Err(_) => return Err(Status::InternalServerError),
    };

    let dbpedia_resource = dbpedia::get_resource(query)
        .await
        .unwrap_or(String::from(""));
    let answer = dbpedia::get_summary(&dbpedia_resource)
        .await
        .unwrap_or(String::from(""));

    Ok(Json(Answer {
        urls,
        small_summary: (&answer).into(),
        answer,
        corrected: query.into(),
    }))
}

#[get("/?<query>&<page>&<language_option>")]
async fn _results(
    state: &State<Config>,
    query: &str,
    page: usize,
    language_option: Option<&str>,
) -> Result<Json<Results>, Status> {
    let page_size = 5;

    match get_url_list(state, query, page, page_size, language_option).await {
        Ok(urls) => Ok(Json(Results { urls })),
        Err(_) => Err(Status::InternalServerError),
    }
}

#[get("/")]
fn _get_peers(state: &State<Config>) -> Result<Json<Peers>, Status> {
    let peers: Vec<Peer> = state
        .peers
        .iter()
        .map(|peer| Peer {
            address: String::from_utf8_lossy(&peer.unwrap().0).parse().unwrap(),
        })
        .collect();

    Ok(Json(Peers { peers }))
}

#[get("/?<address>")]
fn _get_peer(state: &State<Config>, address: &str) -> Result<Json<Peer>, Status> {
    if let Ok(value_result) = state.peers.get(address) {
        if let Some(value_option) = value_result {
            if let Ok(peer) =
                json::from_str::<Peer>(String::from_utf8_lossy(&value_option).as_ref())
            {
                Ok(Json(peer))
            } else {
                Err(Status::InternalServerError)
            }
        } else {
            Err(Status::ExpectationFailed)
        }
    } else {
        Err(Status::NotFound)
    }
}

#[post("/", format = "json", data = "<peer>")]
fn _add_peer(state: &State<Config>, peer: Json<Peer>) -> Status {
    if (&peer).address.starts_with("http://") || (&peer).address.starts_with("https://") {
        return Status::NotAcceptable;
    }

    if let Ok(_) = state
        .peers
        .insert(&peer.address, json::to_string(&peer.0).unwrap().as_str())
    {
        return Status::Accepted;
    } else {
        return Status::NotAcceptable;
    }
}

#[put("/", format = "json", data = "<peer>")]
fn _update_peer(state: &State<Config>, peer: Json<Peer>) -> Status {
    if (&peer).address.starts_with("http://") || (&peer).address.starts_with("https://") {
        return Status::NotAcceptable;
    }

    if let Ok(_) = state
        .peers
        .insert(&peer.address, json::to_string(&peer.0).unwrap().as_str())
    {
        return Status::Accepted;
    } else {
        return Status::NotModified;
    }
}

#[launch]
fn rocket() -> _ {
    let mut p = project_root::get_project_root().unwrap();
    p.push("glove.6B/glove.6B.50d.txt");
    let mut reader = BufReader::new(File::open(p).unwrap());

    let embeddings = Embeddings::read_text(&mut reader).unwrap();
    let db = sled::open("urlDatabase").expect("open");
    let peers = sled::open("peerDatabase").expect("open");
    let mut index = hora::index::hnsw_idx::HNSWIndex::<f32, u128>::new(
        50,
        &hora::index::hnsw_params::HNSWParams::<f32>::default(),
    );

    for url in db.iter() {
        if let Ok(url) = url {
            let url_key: u128 = String::from_utf8_lossy(&url.0).parse().unwrap();
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
        peers,
    };

    rocket::build()
        .manage(config)
        .mount("/_answer", routes![_answer])
        .mount("/_results", routes![_results])
        .mount("/_peers", routes![_get_peers])
        .mount("/_peer", routes![_get_peer, _add_peer, _update_peer])
}
