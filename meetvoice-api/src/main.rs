use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use futures::TryStreamExt;
use mongodb::{bson::doc, options::FindOptions, Client, Collection};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber;

#[derive(Debug, Serialize, Deserialize)]
struct Article {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<mongodb::bson::oid::ObjectId>,
    slug: String,
    titre: String,
    petit_description: Option<String>,
    contenu: Option<String>,
    theme: Option<String>,
    categorie: Option<String>,
    photo: Option<String>,
    photo_description: Option<String>,
    photo_highlight: Option<String>,
    date_publication: Option<String>,
    seo_title: Option<String>,
    seo_description: Option<String>,
    seo_keywords: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct ArticleListItem {
    slug: String,
    titre: String,
    petit_description: Option<String>,
    theme: Option<String>,
    categorie: Option<String>,
    photo: Option<String>,
    date_publication: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    page: Option<u64>,
    limit: Option<i64>,
    categorie: Option<String>,
    theme: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    articles: Vec<ArticleListItem>,
    total: u64,
    page: u64,
    limit: i64,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

type AppState = Arc<Collection<Article>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let mongo_uri = std::env::var("MONGODB_URI")
        .unwrap_or_else(|_| "mongodb://admin:admin123@localhost:27017/articles?authSource=admin".to_string());

    let client = Client::with_uri_str(&mongo_uri)
        .await
        .expect("Failed to connect to MongoDB");

    let db = client.database("articles");
    let collection: Collection<Article> = db.collection("articles");
    let state = Arc::new(collection);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(health))
        .route("/articles", get(list_articles))
        .route("/articles/:slug", get(get_article))
        .layer(cors)
        .with_state(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("MeetVoice API running on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "MeetVoice API OK"
}

async fn list_articles(
    State(collection): State<AppState>,
    Query(params): Query<ListQuery>,
) -> Result<Json<ListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(10).min(50);
    let skip = (page - 1) * limit as u64;

    let mut filter = doc! {};
    if let Some(cat) = params.categorie {
        filter.insert("categorie", cat);
    }
    if let Some(theme) = params.theme {
        filter.insert("theme", doc! { "$regex": theme, "$options": "i" });
    }

    let total = collection
        .count_documents(filter.clone())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e.to_string() }),
            )
        })?;

    let options = FindOptions::builder()
        .sort(doc! { "date_publication": -1 })
        .skip(skip)
        .limit(limit)
        .projection(doc! {
            "slug": 1,
            "titre": 1,
            "petit_description": 1,
            "theme": 1,
            "categorie": 1,
            "photo": 1,
            "date_publication": 1
        })
        .build();

    let cursor = collection.find(filter).with_options(options).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() }),
        )
    })?;

    let articles: Vec<Article> = cursor.try_collect().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() }),
        )
    })?;

    let items: Vec<ArticleListItem> = articles
        .into_iter()
        .map(|a| ArticleListItem {
            slug: a.slug,
            titre: a.titre,
            petit_description: a.petit_description,
            theme: a.theme,
            categorie: a.categorie,
            photo: a.photo,
            date_publication: a.date_publication,
        })
        .collect();

    Ok(Json(ListResponse {
        articles: items,
        total,
        page,
        limit,
    }))
}

async fn get_article(
    State(collection): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Article>, (StatusCode, Json<ErrorResponse>)> {
    let article = collection
        .find_one(doc! { "slug": &slug })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e.to_string() }),
            )
        })?;

    match article {
        Some(a) => Ok(Json(a)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Article '{}' not found", slug),
            }),
        )),
    }
}
