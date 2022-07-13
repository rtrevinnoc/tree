#![feature(once_cell)]
use std::{
    env,
    sync::LazyLock,
    fs::File,
    io::BufReader
};
use finalfusion::{
    vocab::SimpleVocab,
    storage::NdArray,
    embeddings::Embeddings,
    compat::text::ReadText
};
use ndarray::{
    Array,
    CowArray,
    Ix1
};
use faiss::{
    index::IndexImpl,
    Index,
    index_factory,
    MetricType,
    IdMap
};

static EMBEDDINGS: LazyLock<Embeddings<SimpleVocab, NdArray>> = LazyLock::new(|| {
    dbg!(env::current_dir().unwrap());
    let mut reader = BufReader::new(File::open("./glove.6B/glove.6B.50d.txt").unwrap());
    return Embeddings::read_text(&mut reader).unwrap()
});
static INDEX: LazyLock<IdMap<IndexImpl>> = LazyLock::new(|| {
    let index = index_factory(50, "Flat", MetricType::InnerProduct).unwrap();
    return IdMap::new(index).unwrap();
});

pub fn get_word_embedding(word: &str) -> Option<CowArray<f32, Ix1>> {
    return EMBEDDINGS.embedding(word);
}

pub fn get_sentence_embedding(sentence: &str) -> Option<Array<f32, Ix1>> {
    let words: Vec<&str> = sentence.split_whitespace().collect();

    let mut sum_vector = Array::<f32, Ix1>::zeros(50);
    for word in &words {
        match get_word_embedding(word) {
            Some(embedding) => {
                sum_vector = sum_vector + embedding
            },
            None => ()
        }
    }

    if (sum_vector.sum()) == 0.0 {
        return None;
    } else {
        return Some(sum_vector / (words.len() as f32));
    }
}

//let mut index = index_factory(8, "Flat", MetricType::L2)?;
//index.add(my_data)?;
//let result = index.search(my_query, 5)?;
//for (i, (l, d)) in result.labels.iter()
    //.zip(result.distances.iter())
    //.enumerate()
//{
    //println!("#{}: {} (D={})", i + 1, *l, *d);
//}
