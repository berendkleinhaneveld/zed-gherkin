use lsp_server::{Connection, Message, Response};
use lsp_types::{InitializeParams, ServerCapabilities};

#[allow(dead_code)]
mod expression;

fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        references_provider: Some(lsp_types::OneOf::Left(true)),
        ..Default::default()
    };
    let initialize_params = connection.initialize(serde_json::to_value(&capabilities)?)?;
    let _params: InitializeParams = serde_json::from_value(initialize_params)?;

    run_loop(&connection)?;

    io_threads.join()?;
    Ok(())
}

fn run_loop(connection: &Connection) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                let resp = Response {
                    id: req.id,
                    result: Some(serde_json::Value::Null),
                    error: None,
                };
                connection.sender.send(Message::Response(resp))?;
            }
            Message::Notification(_) => {}
            Message::Response(_) => {}
        }
    }
    Ok(())
}
