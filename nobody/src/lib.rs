mod llm;

use godot::classes::INode;
use godot::prelude::*;
use llama_cpp_2::model::LlamaModel;
use llm::run_worker;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use llama_cpp_2::model::LlamaChatMessage;

struct NobodyWhoExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NobodyWhoExtension {}

#[derive(GodotClass)]
#[class(base=Node)]
struct NobodyWhoModel {
    #[export(file)]
    model_path: GString,

    #[export]
    seed: u32,

    model: Option<Arc<LlamaModel>>,
}

#[godot_api]
impl INode for NobodyWhoModel {
    fn init(_base: Base<Node>) -> Self {
        // default values to show in godot editor
        let model_path: String = "model.bin".into();
        let seed = 1234;

        Self {
            model_path: model_path.into(),
            model: None,
            seed,
        }
    }

    fn ready(&mut self) {
        let model_path_string: String = self.model_path.clone().into();
        self.model = Some(llm::get_model(model_path_string.as_str()));
    }
}

#[derive(GodotClass)]
#[class(base=Node)]
struct NobodyWhoPromptCompletion {
    #[export]
    model_node: Option<Gd<NobodyWhoModel>>,

    completion_rx: Option<Receiver<llm::LLMOutput>>,
    prompt_tx: Option<Sender<String>>,

    base: Base<Node>,
}

#[godot_api]
impl INode for NobodyWhoPromptCompletion {
    fn init(base: Base<Node>) -> Self {
        Self {
            model_node: None,
            completion_rx: None,
            prompt_tx: None,
            base,
        }
    }

    fn physics_process(&mut self, _delta: f64) {
        // checks for new tokens from worker thread and emits them as a signal
        loop {
            if let Some(rx) = self.completion_rx.as_ref() {
                match rx.try_recv() {
                    Ok(llm::LLMOutput::Token(token)) => {
                        self.base_mut()
                            .emit_signal("completion_updated".into(), &[Variant::from(token)]);
                    }
                    Ok(llm::LLMOutput::Done) => {
                        self.base_mut()
                            .emit_signal("completion_finished".into(), &[]);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        godot_error!("Unexpected: Model channel disconnected");
                    }
                }
            }
        }
    }
}

#[godot_api]
impl NobodyWhoPromptCompletion {
    #[func]
    fn run(&mut self) {
        if let Some(gd_model_node) = self.model_node.as_mut() {
            let nobody_model: GdRef<NobodyWhoModel> = gd_model_node.bind();
            if let Some(model) = nobody_model.model.clone() {
                // create channels for communicating with worker thread
                let (prompt_tx, prompt_rx) = std::sync::mpsc::channel::<String>();
                let (completion_tx, completion_rx) = std::sync::mpsc::channel::<llm::LLMOutput>();
                self.prompt_tx = Some(prompt_tx);
                self.completion_rx = Some(completion_rx);

                // start worker thread
                let seed = nobody_model.seed;
                std::thread::spawn(move || {
                    run_worker(model, prompt_rx, completion_tx, seed);
                });
            } else {
                godot_error!("Unexpected: Model node is not ready yet.");
            }
        } else {
            godot_error!("Model node not set");
        }
    }

    #[func]
    fn prompt(&mut self, prompt: String) {
        if let Some(tx) = self.prompt_tx.as_ref() {
            tx.send(prompt).unwrap();
        } else {
            godot_error!("Model not initialized. Call `run` first");
        }
    }

    #[signal]
    fn completion_updated();

    #[signal]
    fn completion_finished();
}

#[derive(GodotClass)]
#[class(base=Node)]
struct NobodyWhoPromptChat {
    #[export]
    model_node: Option<Gd<NobodyWhoModel>>,

    #[export]
    player_name: GString,

    #[export]
    npc_name: GString,

    query_tx: Option<Sender<String>>,
    response_rx: Option<Receiver<llm::LLMOutput>>,

    base: Base<Node>,
}

#[godot_api]
impl INode for NobodyWhoPromptChat {
    fn init(base: Base<Node>) -> Self {
        Self {
            model_node: None,
            player_name: "Player".into(),
            npc_name: "Character".into(),
            query_tx: None,
            response_rx: None,
            base,
        }
    }
}

#[godot_api]
impl NobodyWhoPromptChat {
    #[func]
    fn run(&mut self) {
        if let Some(gd_model_node) = self.model_node.as_mut() {
            let nobody_model: GdRef<NobodyWhoModel> = gd_model_node.bind();
            if let Some(model) = nobody_model.model.clone() {
                let (query_tx, query_rx) = std::sync::mpsc::channel::<String>();
                let (response_tx, response_rx) = std::sync::mpsc::channel::<llm::LLMOutput>();

                self.query_tx = Some(query_tx);
                self.response_rx = Some(response_rx);

                let seed = nobody_model.seed;
                std::thread::spawn(move || {
                    run_worker(model, query_rx, response_tx, seed);
                });
            } else {
                godot_error!("Unexpected: Model node is not ready yet.");
            }
        } else {
            godot_error!("Model node not set");
        }
    }

    #[func]
    fn say(&mut self, message: String) {
        // TODO: also send system prompt on first message

        // simple closure that returns Err(String) if something fails
        let say_result = || -> Result<(), String> {
            let tx: &Sender<String> = self.query_tx.as_ref().ok_or(
                "Channel not initialized. Remember to call run() before talking to character."
                    .to_string(),
            )?;
            let gd_model_node = self.model_node.as_mut().ok_or(
                "No model node provided. Remember to set a model node on NobodyWhoPromptChat."
                    .to_string(),
            )?;
            let nobody_model: GdRef<NobodyWhoModel> = gd_model_node.bind();
            let model: Arc<LlamaModel> = nobody_model
                .model
                .clone()
                .ok_or("Could not access LlamaModel from model node.".to_string())?;
            let chatmsg = LlamaChatMessage::new(self.player_name.to_string(), message)
                .map_err(|e| format!("{:?}", e).to_string())?;
            llm::send_chat(model, tx, vec![chatmsg]).unwrap();
            Ok::<(), String>(())
        };

        // run it and show the error in godot if it fails
        if let Err(msg) = say_result() {
            godot_error!("Error sending chat message to model worker: {msg}");
        }
    }

    #[signal]
    fn completion_updated();

    #[signal]
    fn completion_finished();
}
