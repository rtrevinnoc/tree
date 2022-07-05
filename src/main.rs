#[macro_use] extern crate rocket;

use rocket::serde::{Serialize, json::{Json, Value}};
use titlecase::titlecase;

static SPARQL_ENDPOINT: &str = "http://dbpedia.org/sparql";

#[derive(Serialize)]
struct Answer {
    summary: String,
}

async fn get_resource_from_dbpedia(query: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = reqwest::Url::parse_with_params(SPARQL_ENDPOINT, [
        ("query", format!("PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> PREFIX foaf: <http://xmlns.com/foaf/0.1/> PREFIX dbo: <http://dbpedia.org/ontology/>  select ?s WHERE {{ {{ ?s rdfs:label '{}'@en ; a owl:Thing . }} UNION {{ ?altName rdfs:label '{}'@en ; dbo:wikiPageRedirects ?s . }} }}", query, query)),
        ("output", "json".to_string())
    ])?;
    let text = reqwest::get(url).await?.json::<Value>().await?;
    //let var_name = &text["head"]["vars"][0].to_string();
    let resource_redirect = &text["results"]["bindings"][0]["s"]["value"];

    let resource_url: String;
    if resource_redirect.is_null() {
        resource_url = format!("http://dbpedia.org/resource/{}", titlecase(&query).replace(" ","_"));
    } else {
        resource_url = resource_redirect.as_str().unwrap().to_string();
    }
    
    Ok(resource_url)
}

async fn get_summary_from_dbpedia(dbpedia_resource: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = reqwest::Url::parse_with_params(SPARQL_ENDPOINT, [
        ("query", format!("select str(?desc) where {{ <{}> rdfs:comment ?desc filter (langMatches(lang(?desc),'en')) }}", dbpedia_resource)),
        ("output", "json".to_string())
    ])?;
    let text = reqwest::get(url).await?.json::<Value>().await?;
    //let var_name = &text["head"]["vars"][0].to_string();
    
    Ok(text["results"]["bindings"][0]["callret-0"]["value"].as_str().unwrap().to_string())
}

#[get("/?<query>&<page>")]
async fn _answer(query: &str, page: usize) -> Json<Answer> {
    let dbpedia_resource = get_resource_from_dbpedia(query).await.unwrap();
    let summary = get_summary_from_dbpedia(&dbpedia_resource).await.unwrap();
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
