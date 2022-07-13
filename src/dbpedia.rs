use rocket::serde::json::Value;
use titlecase::titlecase;

static SPARQL_ENDPOINT: &str = "http://dbpedia.org/sparql";

pub async fn get_resource(query: &str) -> Result<String, Box<dyn std::error::Error>> {
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

pub async fn get_summary(dbpedia_resource: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = reqwest::Url::parse_with_params(SPARQL_ENDPOINT, [
        ("query", format!("select str(?desc) where {{ <{}> rdfs:comment ?desc filter (langMatches(lang(?desc),'en')) }}", dbpedia_resource)),
        ("output", "json".to_string())
    ])?;
    let text = reqwest::get(url).await?.json::<Value>().await?;
    //let var_name = &text["head"]["vars"][0].to_string();
    
    Ok(text["results"]["bindings"][0]["callret-0"]["value"].as_str().unwrap().to_string())
}
