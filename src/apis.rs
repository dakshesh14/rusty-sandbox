use std::{fs, thread, time::Duration};

use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::sandbox::Sandbox;

#[derive(Deserialize)]
pub struct ExecutionRequest {
    code: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ExecutionResponse {
    output: String,
}

pub async fn run_python(Json(payload): Json<ExecutionRequest>) -> Json<ExecutionResponse> {
    if let Some(sandbox) = Sandbox::new() {
        let timeout = 15;
        let start_time = std::time::Instant::now();

        let filename = format!("/tmp/{}.py", Uuid::new_v4());

        // TODO: delete this file after execution
        if let Err(e) = fs::write(&filename, &payload.code) {
            return Json(ExecutionResponse {
                output: format!("Failed to write code to file: {}", e),
            });
        }

        let command = format!("python3 {}", filename);

        while start_time.elapsed().as_secs() < timeout {
            if sandbox.is_running() {
                match sandbox.run_command(&command) {
                    Ok(output) => {
                        sandbox.terminate();
                        return Json(ExecutionResponse { output });
                    }
                    Err(output) => {
                        sandbox.terminate();
                        return Json(ExecutionResponse { output });
                    }
                }
            }
            thread::sleep(Duration::from_secs(1));
        }

        sandbox.terminate();
        return Json(ExecutionResponse {
            output: "Execution timed out".to_string(),
        });
    }

    Json(ExecutionResponse {
        output: "Failed to create sandbox".to_string(),
    })
}

pub async fn run_cpp(Json(payload): Json<ExecutionRequest>) -> Json<ExecutionResponse> {
    if let Some(sandbox) = Sandbox::new() {
        let timeout = 15;
        let start_time = std::time::Instant::now();

        let id = Uuid::new_v4();
        let source_file = format!("/tmp/{}.cpp", id);
        let binary_file = format!("/tmp/{}", id);

        // TODO: delete this file after execution
        if let Err(e) = fs::write(&source_file, &payload.code) {
            return Json(ExecutionResponse {
                output: format!("Failed to write code to file: {}", e),
            });
        }

        let compile_command = format!("g++ -o {} {}", binary_file, source_file);
        match sandbox.run_command(&compile_command) {
            Ok(_) => {
                let run_command = format!("{}", binary_file);
                while start_time.elapsed().as_secs() < timeout {
                    if sandbox.is_running() {
                        match sandbox.run_command(&run_command) {
                            Ok(output) => {
                                sandbox.terminate();
                                return Json(ExecutionResponse { output });
                            }
                            Err(output) => {
                                sandbox.terminate();
                                return Json(ExecutionResponse { output });
                            }
                        }
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
            Err(output) => {
                sandbox.terminate();
                return Json(ExecutionResponse {
                    output: format!("Compilation failed:\n{}", output),
                });
            }
        }

        sandbox.terminate();
        Json(ExecutionResponse {
            output: "Execution timed out".to_string(),
        })
    } else {
        Json(ExecutionResponse {
            output: "Failed to create sandbox".to_string(),
        })
    }
}

pub fn get_routes() -> Router {
    Router::new()
        .route("/python", post(run_python))
        .route("/cpp", post(run_cpp))
}
