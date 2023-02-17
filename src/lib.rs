#![feature(once_cell)]
use finalfusion::{
    compat::text::ReadText, embeddings::Embeddings, storage::NdArray, vocab::SimpleVocab,
};
use ndarray::{Array, CowArray, Ix1};
use std::{env, fs::File, io::BufReader, sync::LazyLock};

static EMBEDDINGS: LazyLock<Embeddings<SimpleVocab, NdArray>> = LazyLock::new(|| {
    dbg!(env::current_dir().unwrap());
    let mut reader = BufReader::new(File::open("./glove.6B/glove.6B.50d.txt").unwrap());
    return Embeddings::read_text(&mut reader).unwrap();
});

pub fn get_word_embedding(word: &str) -> Option<CowArray<f32, Ix1>> {
    return EMBEDDINGS.embedding(word);
}

pub fn get_sentence_embedding(sentence: &str) -> Option<Array<f32, Ix1>> {
    let words: Vec<&str> = sentence.split_whitespace().collect();

    let mut sum_vector = Array::<f32, Ix1>::zeros(50);
    for word in &words {
        match get_word_embedding(word) {
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
