#[macro_use] extern crate rocket;
use rocket::serde::{
    Serialize,
    json::Json
};
mod dbpedia;

#[derive(Serialize)]
struct Answer {
    summary: String,
}

#[get("/?<query>&<page>")]
async fn _answer(query: &str, page: usize) -> Json<Answer> {
    let dbpedia_resource = dbpedia::get_resource(query).await.unwrap();
    let summary = dbpedia::get_summary(&dbpedia_resource).await.unwrap();
    println!("resource = {}", dbpedia_resource);
    println!("summary = {}", summary);

    Json(Answer {
        summary
    })
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/_answer", routes![_answer])
}
