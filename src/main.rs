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
use std::env::var;
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

#[derive(Serialize, Deserialize)]
struct Peers {
    peers: Vec<Peer>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Peer {
    address: String,
}

struct Config {
    vec_index: hora::index::hnsw_idx::HNSWIndex<f32, u128>,
    db: sled::Db,
    embeddings: Embeddings<SimpleVocab, NdArray>,
    peers: sled::Db,
    http_client: reqwest::Client,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Http error")]
    Reqwest {
        #[from]
        source: reqwest::Error,
    },
    #[error("Serialization error")]
    Json {
        #[from]
        source: json::serde_json::Error,
    },
    #[error("Internal server error")]
    InternalServerError,
    #[error("Expectation failed error")]
    ExpectationFailed,
    #[error("Not acceptable error")]
    NotAcceptable,
    #[error("Not modified error")]
    NotModified,
    #[error("Not found error")]
    NotFound,
    #[error("Bad request error")]
    BadRequest,
    #[error("Unknown error")]
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
        &state.http_client,
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

    let dbpedia_resource = dbpedia::get_resource(&state.http_client, query)
        .await
        .unwrap_or(String::from(""));
    let answer = dbpedia::get_summary(&state.http_client, &dbpedia_resource)
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
        &state.http_client,
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
async fn _add_peer(state: &State<Config>, peer: Json<Peer>) -> Result<Json<Peer>, Error> {
    if !(&peer).address.starts_with("http://") && !(&peer).address.starts_with("https://") {
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
async fn _update_peer(state: &State<Config>, peer: Json<Peer>) -> Result<Json<Peer>, Error> {
    return _add_peer(state, peer).await;
}

#[launch]
async fn rocket() -> _ {
    let mut p = project_root::get_project_root().unwrap();
    p.push("glove.6B/glove.6B.50d.txt");
    let mut reader = BufReader::new(File::open(p).unwrap());

    let http_client = reqwest::Client::new();
    let embeddings = Embeddings::read_text(&mut reader).unwrap();
    let db = sled::open("urlDatabase").expect("open");
    let peers = sled::open("peerDatabase").expect("open");
    let mut vec_index = hora::index::hnsw_idx::HNSWIndex::<f32, u128>::new(
        50,
        &hora::index::hnsw_params::HNSWParams::<f32>::default(),
    );

    let this_peer = Peer {
        address: var("PEAR_ADDRESS").unwrap(),
    };

    let mut peer_list: Vec<Peer> = match var("PEAR_SYNC_WITH") {
        Ok(address) => match http_client.get(format!("{}/_peers", address)).send().await {
            Ok(json_value) => match json_value.json::<Peers>().await {
                Ok(peers_json) => peers_json.peers,
                Err(e) => {
                    println!("Error: {:?}. Deserialization error while fetching peer.", e);
                    vec![]
                }
            },
            Err(e) => {
                println!("Error: {:?}. Error fetching peer.", e);
                vec![]
            }
        },
        Err(e) => {
            println!("Error: {:?}. Set the PEAR_SYNC_WITH environment variable to where you want to the serer with which you want to sync peers with.", e);
            vec![]
        }
    };

    peer_list.push(this_peer.clone());
    for peer in peer_list.iter() {
        match peers.insert(&peer.address, json::to_string(peer).unwrap().as_str()) {
            Ok(_) => {
                if peer.address == this_peer.address {
                    continue;
                }

                match http_client
                    .post(format!("{}/_peer", &peer.address))
                    .header("Content-Type", "application/json")
                    .body(json::to_string(&this_peer).unwrap())
                    .send()
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        println!(
                            "Error: {:?}. Error requesting this peer registration at another peer.",
                            e
                        );
                    }
                };
            }
            Err(e) => {
                println!("Error: {:?}. Error inserting peer into peer database.", e);
            }
        }
    }

    for url in db.iter() {
        if let Ok(url) = url {
            let url_key: u128 = String::from_utf8_lossy(&url.0).parse().unwrap();
            match json::from_str::<CrawledEntry>(String::from_utf8_lossy(&url.1).as_ref()) {
                Ok(url_value) => {
                    vec_index.add(&url_value.vec, url_key).unwrap();
                }
                Err(e) => {
                    println!("Error: {:?}. URL database deserialization error.", e);
                }
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
        http_client,
    };

    rocket::build()
        .manage(config)
        .mount("/_answer", routes![_answer])
        .mount("/_results", routes![_results])
        .mount("/_peers", routes![_get_peers])
        .mount("/_peer", routes![_get_peer, _add_peer, _update_peer])
}
