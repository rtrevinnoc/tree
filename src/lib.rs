use finalfusion::{embeddings::Embeddings, storage::NdArray, vocab::SimpleVocab};
use ndarray::{Array, CowArray, Ix1};
use rocket::serde::{Deserialize, Serialize};
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
