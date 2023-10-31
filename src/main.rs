#[macro_use]
extern crate rocket;
use finalfusion::{
    compat::text::ReadText, embeddings::Embeddings, storage::NdArray, vocab::SimpleVocab,
};
use hora::core::ann_index::ANNIndex;
use rocket::http::Status;
use rocket::response::{self, Responder};
use rocket::serde::{json, json::Json, Deserialize, Serialize};
use rocket::{Request, State};
use std::{fs::File, io::BufReader};
use thiserror::Error;
use tree::{get_url_list, CrawledEntry, Url};
mod dbpedia;

#[derive(Serialize)]
struct Answer {
    answer: String,
    urls: Vec<Url>,
    small_summary: String,
    corrected: String,
}

#[derive(Serialize, Deserialize)]
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

struct Config {
    vec_index: hora::index::hnsw_idx::HNSWIndex<f32, u128>,
    db: sled::Db,
    embeddings: Embeddings<SimpleVocab, NdArray>,
    peers: sled::Db,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("http error")]
    Reqwest {
        #[from]
        source: reqwest::Error,
    },
    #[error("serialization error")]
    Json {
        #[from]
        source: json::serde_json::Error,
    },
    #[error("internal server error")]
    InternalServerError,
    #[error("expectation failed error")]
    ExpectationFailed,
    #[error("not acceptable error")]
    NotAcceptable,
    #[error("not modified error")]
    NotModified,
    #[error("not found error")]
    NotFound,
    #[error("bad request error")]
    BadRequest,
    #[error("unknown error")]
    Unknown,
}

impl<'r, 'o: 'r> Responder<'r, 'o> for Error {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'o> {
        match self {
            Self::InternalServerError => Status::InternalServerError.respond_to(req),
            Self::ExpectationFailed => Status::ExpectationFailed.respond_to(req),
            Self::NotFound => Status::NotFound.respond_to(req),
            Self::NotModified => Status::NotModified.respond_to(req),
            Self::NotAcceptable => Status::NotAcceptable.respond_to(req),
            Self::BadRequest => Status::BadRequest.respond_to(req),
            _ => Status::InternalServerError.respond_to(req),
        }
    }
}

#[get("/?<query>&<page>&<language_option>")]
async fn _answer(
    state: &State<Config>,
    query: &str,
    page: usize,
    language_option: Option<&str>,
) -> Result<Json<Answer>, Error> {
    let page_size = 5;

    let urls = match get_url_list(
        &state.embeddings,
        &state.vec_index,
        &state.db,
        query,
        page,
        page_size,
        language_option,
    )
    .await
    {
        Ok(results) => results,
        Err(_) => return Err(Error::InternalServerError),
    };

    // for peer in state.peers.iter() {
    //     if let Ok(peer) = peer {
    //         let peer_key: u128 = String::from_utf8_lossy(&peer.0).parse().unwrap();
    //         match json::from_str::<Peer>(String::from_utf8_lossy(&peer.1).as_ref()) {
    //             Ok(_peer_value) => {
    //                 let response = reqwest::get(format!(
    //                     "{}/_results?query={}&page={}",
    //                     peer_key, query, page
    //                 ))
    //                 .await?
    //                 .text()
    //                 .await?;
    //
    //                 let mut results = json::from_str::<Results>(&response)?;
    //                 urls.append(&mut results.urls)
    //             }
    //             Err(_) => return Err(Error::InternalServerError),
    //         }
    //     }
    // }

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
) -> Result<Json<Results>, Error> {
    let page_size = 5;

    match get_url_list(
        &state.embeddings,
        &state.vec_index,
        &state.db,
        query,
        page,
        page_size,
        language_option,
    )
    .await
    {
        Ok(urls) => Ok(Json(Results { urls })),
        Err(_) => Err(Error::InternalServerError),
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
fn _get_peer(state: &State<Config>, address: &str) -> Result<Json<Peer>, Error> {
    if let Ok(value_result) = state.peers.get(address) {
        if let Some(value_option) = value_result {
            if let Ok(peer) =
                json::from_str::<Peer>(String::from_utf8_lossy(&value_option).as_ref())
            {
                Ok(Json(peer))
            } else {
                Err(Error::InternalServerError)
            }
        } else {
            Err(Error::InternalServerError)
        }
    } else {
        Err(Error::NotFound)
    }
}

#[post("/", format = "json", data = "<peer>")]
fn _add_peer(state: &State<Config>, peer: Json<Peer>) -> Result<Json<Peer>, Error> {
    if !(&peer).address.starts_with("http://") || !(&peer).address.starts_with("https://") {
        return Err(Error::BadRequest);
    }

    if let Ok(_) = state
        .peers
        .insert(&peer.address, json::to_string(&peer.0).unwrap().as_str())
    {
        return Ok(peer);
    } else {
        return Err(Error::InternalServerError);
    }
}

#[put("/", format = "json", data = "<peer>")]
fn _update_peer(state: &State<Config>, peer: Json<Peer>) -> Result<Json<Peer>, Error> {
    if !(&peer).address.starts_with("http://") || !(&peer).address.starts_with("https://") {
        return Err(Error::BadRequest);
    }

    if let Ok(_) = state
        .peers
        .insert(&peer.address, json::to_string(&peer.0).unwrap().as_str())
    {
        return Ok(peer);
    } else {
        return Err(Error::InternalServerError);
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
    let mut vec_index = hora::index::hnsw_idx::HNSWIndex::<f32, u128>::new(
        50,
        &hora::index::hnsw_params::HNSWParams::<f32>::default(),
    );

    for url in db.iter() {
        if let Ok(url) = url {
            let url_key: u128 = String::from_utf8_lossy(&url.0).parse().unwrap();
            match json::from_str::<CrawledEntry>(String::from_utf8_lossy(&url.1).as_ref()) {
                Ok(url_value) => {
                    vec_index.add(&url_value.vec, url_key).unwrap();
                }
                Err(_) => {}
            }
        }
    }

    vec_index
        .build(hora::core::metrics::Metric::CosineSimilarity)
        .unwrap();

    let config = Config {
        vec_index,
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
