#[tokio::main(worker_threads = 2)]
async fn main() {
    if std::env::args().any(|arg| arg == "--build-info") {
        println!(
            "{}",
            serde_json::json!({"fixtureHttp":cfg!(feature="fixture-http")})
        );
        return;
    }
    if let Err(error) = tezbar_extension_sdk::serve(tezbar_gif_search::GifSearch).await {
        eprintln!("GIF Search: {error}");
        std::process::exit(1);
    }
}
