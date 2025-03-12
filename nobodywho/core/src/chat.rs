use crate::chat_state;
use crate::llm;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, trace};

#[derive(Debug, thiserror::Error)]
pub enum ChatLoopError {
    // see Issue #104 for why this error message is so long.
    // https://github.com/nobodywho-ooo/nobodywho/issues/96
    #[error(
        "Lama.cpp failed fetching chat template from the model file. \
        This is likely because you're using an older GGUF file, \
        which might not include a chat template. \
        For example, this is the case for most LLaMA2-based GGUF files. \
        Try using a more recent GGUF model file. \
        If you want to check if a given model includes a chat template, \
        you can use the gguf-dump script from llama.cpp. \
        Here is a more technical detailed error: {0}"
    )]
    ChatTemplateError(#[from] chat_state::FromModelError),

    #[error("Failed initializing the LLM worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Worker died while generating response: {0}")]
    GenerateResponseError(#[from] llm::GenerateResponseError),

    #[error("Worker finished stream without a complete response")]
    NoResponseError,
}

pub trait ChatOutput {
    fn emit_token(&self, token: String);
    fn emit_response(&self, resp: String);
    fn emit_error(&self, err: String);
}

pub async fn simple_chat_loop(
    params: llm::LLMActorParams,
    system_prompt: String,
    mut say_rx: mpsc::Receiver<String>,
    output: Box<dyn ChatOutput>,
) -> Result<(), ChatLoopError> {
    info!("Entering simple chat loop");

    // init chat state
    let mut chat_state = chat_state::ChatState::from_model(&params.model)?;
    chat_state.add_message("system".to_string(), system_prompt);
    info!("Initialized chat state.");

    // init actor
    let actor = llm::LLMActorHandle::new(params).await?;
    info!("Initialized actor.");

    // wait for message from user
    while let Some(message) = say_rx.recv().await {
        chat_state.add_message("user".to_string(), message);
        let diff = chat_state.render_diff().expect("TODO: handle err");

        // stream out the response
        let full_response = actor
            .generate_response(diff)
            .await
            .fold(None, |_, out| {
                debug!("Streamed out: {out:?}");
                match out {
                    Ok(llm::WriteOutput::Token(token)) => {
                        trace!("Got new token: {token:?}");
                        output.emit_token(token);
                        None
                    }
                    Err(err) => {
                        error!("Got error from worker: {err:?}");
                        output.emit_error(format!("{err:?}"));
                        Some(Err(err))
                    }
                    Ok(llm::WriteOutput::Done(resp)) => Some(Ok(resp)),
                }
            })
            .await
            .ok_or(ChatLoopError::NoResponseError)??;

        // we have a full response. send it out.
        output.emit_response(full_response.clone());
        chat_state.add_message("assistant".to_string(), full_response);
        let _ = chat_state.render_diff();
    }

    // XXX: we only arrive here when the sender-part of the say channel is dropped
    // and in that case, we don't have anything to send our error to anyway
    Ok(()) // accept our fate
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingLoopError {
    #[error("Failed initializing the LLM worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Failed generating embedding: {0}")]
    GenerateEmbeddingError(#[from] llm::GenerateEmbeddingError),
}

pub trait EmbeddingOutput {
    fn emit_embedding(&self, embd: Vec<f32>);
}

pub async fn simple_embedding_loop(
    params: llm::LLMActorParams,
    mut text_rx: mpsc::Receiver<String>,
    output: Box<dyn EmbeddingOutput>,
) -> Result<(), EmbeddingLoopError> {
    let actor = llm::LLMActorHandle::new(params).await?;
    while let Some(text) = text_rx.recv().await {
        let embd = actor.generate_embedding(text).await?;
        output.emit_embedding(embd);
    }
    Ok(()) // we dead
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::get_model;
    use crate::sampler_config::SamplerConfig;

    macro_rules! test_model_path {
        () => {
            std::env::var("TEST_MODEL")
                .unwrap_or("model.gguf".to_string())
                .as_str()
        };
    }

    struct MockOutput {
        response_tx: mpsc::Sender<String>,
    }

    impl MockOutput {
        fn new() -> (Self, mpsc::Receiver<String>) {
            let (response_tx, response_rx) = mpsc::channel(1024);
            (Self { response_tx }, response_rx)
        }
    }

    impl ChatOutput for MockOutput {
        fn emit_response(&self, resp: String) {
            self.response_tx.try_send(resp).expect("send failed!");
        }
        fn emit_token(&self, token: String) {
            debug!("MockEngine: {token}");
        }
        fn emit_error(&self, err: String) {
            error!("MockEngine: {err}");
            panic!()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_actor_chat() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_timer(tracing_subscriber::fmt::time::uptime())
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE) // Shows timing on span close
            .init();

        // Setup
        let model = get_model(test_model_path!(), true).unwrap();
        let system_prompt =
            "You are a helpful assistant. The user asks you a question, and you provide an answer."
                .to_string();
        let params = llm::LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            stop_tokens: vec![],
            use_embeddings: false,
        };

        let (mock_output, mut response_rx) = MockOutput::new();
        let (say_tx, say_rx) = mpsc::channel(2);

        let local = tokio::task::LocalSet::new();
        local.spawn_local(simple_chat_loop(
            params,
            system_prompt,
            say_rx,
            Box::new(mock_output),
        ));

        let check_results = async move {
            let _ = say_tx
                .send("What is the capital of Denmark?".to_string())
                .await;
            let response = response_rx.recv().await.unwrap();
            assert!(
                response.contains("Copenhagen"),
                "Expected completion to contain 'Copenhagen', got: {response}"
            );

            let _ = say_tx
                .send("What language do they speak there?".to_string())
                .await;
            let response = response_rx.recv().await.unwrap();

            assert!(
                response.contains("Danish"),
                "Expected completion to contain 'Danish', got: {response}"
            );
        };

        // run stuff
        local.run_until(check_results).await;
    }
}
