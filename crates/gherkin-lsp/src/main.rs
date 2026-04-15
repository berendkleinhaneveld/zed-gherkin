mod expression;
mod indexer;
mod server;

fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    server::run()
}
