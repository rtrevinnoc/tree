use finalfusion::{embeddings::Embeddings, storage::NdArray, vocab::SimpleVocab};
use ndarray::{Array, CowArray, Ix1};
use rocket::serde::{Deserialize, Serialize};

// static EMBEDDINGS: LazyLock<Embeddings<SimpleVocab, NdArray>> = LazyLock::new(|| {
//     let mut p = project_root::get_project_root().unwrap();
//     p.push("glove/glove.6B.50d.txt");
//     let mut reader = BufReader::new(File::open("glove.6B/glove.6B.50d.txt").unwrap());
//     return Embeddings::read_text(&mut reader).unwrap();
// });

#[derive(Serialize, Deserialize)]
pub struct CrawledEntry {
    pub url: String,
    pub title: String,
    pub header: String,
    pub description: String,
    pub vec: Vec<f32>,
}

pub trait Embedding {
    fn get_word_embedding(&self, word: &str) -> Option<CowArray<f32, Ix1>>;
    fn get_sentence_embedding(&self, sentence: &str) -> Option<Array<f32, Ix1>>;
}

impl Embedding for Embeddings<SimpleVocab, NdArray> {
    fn get_word_embedding(&self, word: &str) -> Option<CowArray<f32, Ix1>> {
        return self.embedding(word);
    }

    fn get_sentence_embedding(&self, sentence: &str) -> Option<Array<f32, Ix1>> {
        let words: Vec<&str> = sentence.split_whitespace().collect();

        let mut sum_vector = Array::<f32, Ix1>::zeros(50);
        for word in &words {
            match self.get_word_embedding(word) {
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
}
