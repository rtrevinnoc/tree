use finalfusion::{embeddings::Embeddings, storage::NdArray, vocab::SimpleVocab};
use hora::core::ann_index::ANNIndex;
use ndarray::{Array, CowArray, Ix1};
use rocket::serde::{json, Deserialize, Serialize};
use rocket::State;
mod dbpedia;

#[derive(Serialize, Deserialize)]
pub struct CrawledEntry {
    pub url: String,
    pub title: String,
    pub header: String,
    pub description: String,
    pub vec: Vec<f32>,
    pub language: String,
}

#[derive(Serialize)]
pub struct Url {
    pub url: String,
    pub title: String,
    pub header: String,
    pub description: String,
    pub language: String,
}

pub struct Config {
    pub vec_index: hora::index::hnsw_idx::HNSWIndex<f32, u128>,
    pub db: sled::Db,
    pub embeddings: Embeddings<SimpleVocab, NdArray>,
    pub peers: sled::Db,
}

pub fn get_word_embedding<'a>(
    embeddings: &'a Embeddings<SimpleVocab, NdArray>,
    word: &'a str,
) -> Option<CowArray<'a, f32, Ix1>> {
    return embeddings.embedding(word.to_lowercase().as_ref());
}

pub fn get_chunk_embedding(
    embeddings: &Embeddings<SimpleVocab, NdArray>,
    sentence: &str,
) -> Option<Array<f32, Ix1>> {
    let words: Vec<&str> = sentence.split_whitespace().collect();

    let mut sum_vector = Array::<f32, Ix1>::zeros(50);
    for word in &words {
        match get_word_embedding(&embeddings, word) {
            Some(embedding) => sum_vector = sum_vector + embedding,
            None => (),
        }
    }

    if (sum_vector.sum()) == 0.0 {
        return None;
    } else {
        return Some(sum_vector / (words.len() as f32));
    }
}

pub async fn get_sentence_embedding(
    embeddings: &Embeddings<SimpleVocab, NdArray>,
    sentence: &str,
) -> Option<Array<f32, Ix1>> {
    let words: Vec<&str> = sentence.split_whitespace().collect();

    let mut sum_vector = Array::<f32, Ix1>::zeros(50);
    for word in &words {
        match get_word_embedding(embeddings, word) {
            Some(embedding) => sum_vector = sum_vector + embedding,
            None => {
                if let Ok(dbpedia_resource) = dbpedia::get_resource(word).await {
                    if let Ok(dbpedia_summary) = dbpedia::get_summary(&dbpedia_resource).await {
                        if let Some(embedding) = get_chunk_embedding(embeddings, &dbpedia_summary) {
                            sum_vector = sum_vector + embedding;
                        }
                    }
                }
            }
        }
    }

    if (sum_vector.sum()) == 0.0 {
        return None;
    } else {
        return Some(sum_vector / (words.len() as f32));
    }
}

pub async fn get_url_list(
    state: &State<Config>,
    query: &str,
    page: usize,
    page_size: usize,
    language_option: Option<&str>,
) -> Result<Vec<Url>, ()> {
    let mut urls: Vec<Url> = Vec::new();
    if let Some(query_vec) = get_sentence_embedding(&state.embeddings, query).await {
        for vec_id in state
            .vec_index
            .search(&query_vec.to_vec(), page_size * page)
            .split_off(page_size * (page - 1))
        {
            if let Ok(value_result) = state.db.get(&vec_id.to_string()) {
                if let Some(value_option) = value_result {
                    match json::from_str::<CrawledEntry>(
                        String::from_utf8_lossy(&value_option).as_ref(),
                    ) {
                        Ok(url_value) => {
                            if let Some(language) = language_option {
                                if !url_value.language.eq(language) {
                                    continue;
                                }
                            }

                            urls.push(Url {
                                url: url_value.url,
                                title: url_value.title,
                                header: url_value.header,
                                description: url_value.description,
                                language: url_value.language,
                            });
                        }
                        Err(_) => return Err(()),
                    }
                }
            }
        }
    }

    Ok(urls)
}
