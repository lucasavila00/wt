use super::{map_store_error, AgentToolGateway, Service};
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use wt_control_protocol::{
    ApiError, ErrorCode, Response, StartWindow, Window, WindowId, WindowOutputChannel,
    WindowOutputRecord, WindowScreen, WindowState, WorldStatus, MAX_WINDOW_INPUT_BYTES,
    MAX_WINDOW_OUTPUT_LIMIT,
};
use wt_guest::{WindowLaunch, WindowObservation, WorldWorker};
use wt_workload_registry::{NewWindow, StoredWindow};

pub(super) fn is_window_operation(operation: &wt_control_protocol::Operation) -> bool {
    matches!(
        operation,
        wt_control_protocol::Operation::StartWindow(_)
            | wt_control_protocol::Operation::GetWindow { .. }
            | wt_control_protocol::Operation::SendWindowInput { .. }
            | wt_control_protocol::Operation::StopWindow { .. }
            | wt_control_protocol::Operation::DeleteWindow { .. }
    )
}

pub(super) fn is_public_operation(operation: &wt_control_protocol::Operation) -> bool {
    matches!(
        operation,
        wt_control_protocol::Operation::CreateWorld(_)
            | wt_control_protocol::Operation::DeleteWorld { .. }
    ) || is_window_operation(operation)
}

pub(super) fn prepare_public_operation(
    owner: &str,
    request_id: uuid::Uuid,
    operation: wt_control_protocol::Operation,
) -> Result<wt_control_protocol::Operation, ApiError> {
    Ok(match operation {
        wt_control_protocol::Operation::StartWindow(mut request) => {
            request.window_id = Some(deterministic_window_id(owner, request_id));
            request.control_token = Some(generate_control_token()?);
            wt_control_protocol::Operation::StartWindow(request)
        }
        wt_control_protocol::Operation::SendWindowInput {
            window_id,
            control_token,
            data,
            ..
        } => wt_control_protocol::Operation::SendWindowInput {
            window_id,
            control_token,
            data,
            api_request_id: Some(request_id),
        },
        operation => operation,
    })
}

pub(super) fn absent_deletion_response(
    operation: &wt_control_protocol::Operation,
) -> Option<Response> {
    match operation {
        wt_control_protocol::Operation::DeleteWorld { world_id } => Some(Response::WorldDeleted {
            world_id: *world_id,
        }),
        wt_control_protocol::Operation::DeleteWindow { window_id, .. } => {
            Some(Response::WindowDeleted {
                window_id: *window_id,
            })
        }
        _ => None,
    }
}

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn execute_public_read(
        &self,
        owner: &str,
        request_id: uuid::Uuid,
        server_id: uuid::Uuid,
        operation: wt_control_protocol::Operation,
        progress: &mut dyn std::io::Write,
    ) -> wt_control_protocol::ApiResponse {
        let outcome = match self.execute_with_progress(owner, operation, progress) {
            Ok(response) => wt_control_protocol::Outcome::Ok {
                response: Box::new(response),
            },
            Err(error) => wt_control_protocol::Outcome::Error { error },
        };
        wt_control_protocol::ApiResponse::from_outcome(outcome)
            .with_request_metadata(request_id, server_id, None)
    }

    pub(super) fn stop_windows_for_world(
        &self,
        world_id: wt_control_protocol::WorldId,
        running: bool,
    ) -> Result<(), ApiError> {
        let has_windows = !self
            .store
            .windows_for_world(world_id)
            .map_err(map_store_error)?
            .is_empty();
        if running && has_windows {
            self.worker.stop_world_windows(world_id).map_err(|error| {
                ApiError::new(ErrorCode::Backend, format!("stop world windows: {error}"))
            })?;
        }
        self.store
            .mark_world_windows_stopped(world_id)
            .map_err(map_store_error)
    }
    pub(super) fn execute_window_operation(
        &self,
        owner: &str,
        operation: wt_control_protocol::Operation,
    ) -> Result<Response, ApiError> {
        let world_id = match &operation {
            wt_control_protocol::Operation::StartWindow(request) => request.world_id,
            wt_control_protocol::Operation::GetWindow { window_id, .. }
            | wt_control_protocol::Operation::SendWindowInput { window_id, .. }
            | wt_control_protocol::Operation::StopWindow { window_id, .. }
            | wt_control_protocol::Operation::DeleteWindow { window_id, .. } => {
                self.store
                    .get_owned_window(owner, *window_id)
                    .map_err(map_store_error)?
                    .world_id
            }
            _ => unreachable!("caller checks the operation"),
        };
        let _operation = self.operations.try_lock_world(world_id).ok_or_else(|| {
            ApiError::new(ErrorCode::Conflict, "world operation is active").retryable()
        })?;
        match operation {
            wt_control_protocol::Operation::StartWindow(request) => {
                self.start_window(owner, request)
            }
            wt_control_protocol::Operation::GetWindow {
                window_id,
                after,
                limit,
                include_screen,
            } => self.get_window(owner, window_id, after, limit, include_screen),
            wt_control_protocol::Operation::SendWindowInput {
                window_id,
                control_token,
                data,
                api_request_id,
            } => self.send_window_input(owner, window_id, &control_token, &data, api_request_id),
            wt_control_protocol::Operation::StopWindow {
                window_id,
                control_token,
            } => self.stop_window(owner, window_id, &control_token),
            wt_control_protocol::Operation::DeleteWindow {
                window_id,
                control_token,
            } => self.delete_window(owner, window_id, &control_token),
            _ => unreachable!("caller checks the operation"),
        }
    }
    pub(super) fn start_window(
        &self,
        owner: &str,
        request: StartWindow,
    ) -> Result<Response, ApiError> {
        request
            .validate()
            .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error))?;
        let world = self
            .store
            .get_owned_by_id(owner, request.world_id)
            .map_err(map_store_error)?;
        if world.world.status != WorldStatus::Running {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!("world is {}; expected running", world.world.status),
            ));
        }
        let window_id = request.window_id.ok_or_else(|| {
            ApiError::new(
                ErrorCode::Internal,
                "start window identity was not assigned",
            )
        })?;
        let proposed_control_token = request.control_token.ok_or_else(|| {
            ApiError::new(
                ErrorCode::Internal,
                "start window control token was not assigned",
            )
        })?;
        let control_token = match self.store.get_owned_window(owner, window_id) {
            Ok(existing)
                if existing.world_id == request.world_id
                    && existing.argv == request.argv
                    && existing.cwd == request.cwd =>
            {
                existing.control_token
            }
            Ok(_) => {
                return Err(ApiError::new(
                    ErrorCode::Conflict,
                    "window identity already has different launch inputs",
                ))
            }
            Err(wt_workload_registry::StoreError::NotFound) => {
                self.store
                    .insert_window(&NewWindow {
                        window_id,
                        world_id: request.world_id,
                        owner: owner.to_owned(),
                        tmux_window_id: None,
                        control_token: proposed_control_token.clone(),
                        control_token_hash: token_hash(&proposed_control_token),
                        argv: request.argv.clone(),
                        cwd: request.cwd.clone(),
                    })
                    .map_err(map_store_error)?;
                proposed_control_token
            }
            Err(error) => return Err(map_store_error(error)),
        };
        let started = self
            .worker
            .start_window(
                request.world_id,
                &WindowLaunch {
                    window_id,
                    argv: request.argv.clone(),
                    cwd: request.cwd.clone(),
                },
            )
            .map_err(|error| backend("start window", error))?;
        if !valid_tmux_window_id(&started.tmux_window_id) {
            return Err(ApiError::new(
                ErrorCode::Backend,
                "guest returned an invalid tmux window ID",
            ));
        }
        self.store
            .activate_window(window_id, &started.tmux_window_id)
            .map_err(map_store_error)?;
        let window = Window {
            window_id,
            world_id: request.world_id,
            state: WindowState::Running,
            exit_code: None,
            exit_signal: None,
            output: vec![],
            next_after: 0,
            oldest_available: 1,
            output_gap: false,
            screen: None,
        };
        Ok(Response::WindowStarted {
            window: Box::new(window),
            control_token,
        })
    }

    pub(super) fn get_window(
        &self,
        owner: &str,
        window_id: WindowId,
        after: u64,
        limit: u32,
        include_screen: bool,
    ) -> Result<Response, ApiError> {
        if limit > MAX_WINDOW_OUTPUT_LIMIT {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                format!("window output limit exceeds {MAX_WINDOW_OUTPUT_LIMIT}"),
            ));
        }
        Ok(Response::Window {
            window: Box::new(self.get_window_value(
                owner,
                window_id,
                after,
                limit,
                include_screen,
            )?),
        })
    }

    pub(super) fn send_window_input(
        &self,
        owner: &str,
        window_id: WindowId,
        control_token: &str,
        data: &[u8],
        api_request_id: Option<uuid::Uuid>,
    ) -> Result<Response, ApiError> {
        if data.len() > MAX_WINDOW_INPUT_BYTES {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                format!("window input exceeds {MAX_WINDOW_INPUT_BYTES} bytes"),
            ));
        }
        let window = self.authorize_control(owner, window_id, control_token)?;
        if window.state != wt_workload_registry::WindowState::Running {
            return Err(ApiError::new(ErrorCode::Conflict, "window is not running"));
        }
        let api_request_id = api_request_id.ok_or_else(|| {
            ApiError::new(
                ErrorCode::Internal,
                "input request identity was not assigned",
            )
        })?;
        let sequence_id = self
            .store
            .enqueue_window_input(window_id, api_request_id, data)
            .map_err(map_store_error)?;
        // The durable queue is the acknowledgement boundary. A later read retries delivery.
        let _ = self.drain_window_input(&window);
        Ok(Response::WindowInputAccepted {
            window_id,
            sequence_id,
        })
    }

    pub(super) fn stop_window(
        &self,
        owner: &str,
        window_id: WindowId,
        control_token: &str,
    ) -> Result<Response, ApiError> {
        let window = self.authorize_control(owner, window_id, control_token)?;
        if window.state == wt_workload_registry::WindowState::Running {
            self.worker
                .stop_window(window.world_id, window_id)
                .map_err(|error| backend("stop window", error))?;
            self.store
                .update_window_observation(
                    window_id,
                    wt_workload_registry::WindowState::Stopped,
                    None,
                    None,
                    window.screen.as_deref(),
                    window.screen_observed_at_unix_ms,
                )
                .map_err(map_store_error)?;
        }
        Ok(Response::WindowStopped { window_id })
    }

    pub(super) fn delete_window(
        &self,
        owner: &str,
        window_id: WindowId,
        control_token: &str,
    ) -> Result<Response, ApiError> {
        let window = self.authorize_control(owner, window_id, control_token)?;
        self.worker
            .delete_window(window.world_id, window_id)
            .map_err(|error| backend("delete window", error))?;
        self.store
            .delete_owned_window(owner, window_id)
            .map_err(map_store_error)?;
        Ok(Response::WindowDeleted { window_id })
    }

    fn get_window_value(
        &self,
        owner: &str,
        window_id: WindowId,
        after: u64,
        limit: u32,
        include_screen: bool,
    ) -> Result<Window, ApiError> {
        let mut stored = self
            .store
            .get_owned_window(owner, window_id)
            .map_err(map_store_error)?;
        let _ = self.drain_window_input(&stored);
        let observation = self
            .worker
            .observe_window(stored.world_id, window_id, stored.output_offset)
            .map_err(|error| backend("observe window", error))?;
        if stored.tmux_window_id.as_deref() != Some(&observation.tmux_window_id) {
            return Err(ApiError::new(
                ErrorCode::Backend,
                "guest window identity changed",
            ));
        }
        self.record_observation(&stored, &observation)?;
        stored = self
            .store
            .get_owned_window(owner, window_id)
            .map_err(map_store_error)?;
        let page = self
            .store
            .window_output(window_id, after, limit)
            .map_err(map_store_error)?;
        Ok(Window {
            window_id,
            world_id: stored.world_id,
            state: state(stored.state),
            exit_code: stored.exit_code,
            exit_signal: stored.exit_signal,
            output: page
                .output
                .into_iter()
                .map(|record| WindowOutputRecord {
                    record_id: record.record_id,
                    channel: if record.channel == "stdout" {
                        WindowOutputChannel::Stdout
                    } else {
                        WindowOutputChannel::Stderr
                    },
                    data: record.data,
                })
                .collect(),
            next_after: page.next_after,
            oldest_available: page.oldest_available,
            output_gap: page.gap,
            screen: include_screen
                .then(|| {
                    Some(WindowScreen {
                        text: stored.screen?,
                        observed_at_unix_ms: stored.screen_observed_at_unix_ms?,
                    })
                })
                .flatten(),
        })
    }

    fn record_observation(
        &self,
        stored: &StoredWindow,
        observation: &WindowObservation,
    ) -> Result<(), ApiError> {
        let records = observation
            .output
            .iter()
            .map(|record| {
                (
                    match record.channel {
                        WindowOutputChannel::Stdout => "stdout".to_owned(),
                        WindowOutputChannel::Stderr => "stderr".to_owned(),
                    },
                    record.data.clone(),
                )
            })
            .collect::<Vec<_>>();
        self.store
            .record_window_observation(
                stored.window_id,
                stored.output_offset,
                observation.output_offset,
                &records,
                match observation.state {
                    WindowState::Running => wt_workload_registry::WindowState::Running,
                    WindowState::Exited => wt_workload_registry::WindowState::Exited,
                    WindowState::Stopped => wt_workload_registry::WindowState::Stopped,
                },
                observation.exit_code,
                observation.exit_signal,
                &observation.screen,
                observation.screen_observed_at_unix_ms,
            )
            .map_err(map_store_error)
    }

    fn drain_window_input(&self, window: &StoredWindow) -> Result<(), ApiError> {
        for input in self
            .store
            .pending_window_input(window.window_id)
            .map_err(map_store_error)?
        {
            self.worker
                .send_window_input(
                    window.world_id,
                    window.window_id,
                    input.sequence_id,
                    &input.data,
                )
                .map_err(|error| backend("send window input", error))?;
            self.store
                .acknowledge_window_input(window.window_id, input.sequence_id)
                .map_err(map_store_error)?;
        }
        Ok(())
    }

    fn authorize_control(
        &self,
        owner: &str,
        window_id: WindowId,
        control_token: &str,
    ) -> Result<StoredWindow, ApiError> {
        let window = self
            .store
            .get_owned_window(owner, window_id)
            .map_err(map_store_error)?;
        if !constant_time_eq(
            token_hash(control_token).as_bytes(),
            window.control_token_hash.as_bytes(),
        ) {
            return Err(ApiError::new(ErrorCode::NotFound, "window not found"));
        }
        Ok(window)
    }
}

fn state(state: wt_workload_registry::WindowState) -> WindowState {
    match state {
        wt_workload_registry::WindowState::Starting => WindowState::Running,
        wt_workload_registry::WindowState::Running => WindowState::Running,
        wt_workload_registry::WindowState::Exited => WindowState::Exited,
        wt_workload_registry::WindowState::Stopped => WindowState::Stopped,
    }
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn deterministic_window_id(owner: &str, request_id: uuid::Uuid) -> WindowId {
    let digest = Sha256::digest([owner.as_bytes(), request_id.as_bytes()].concat());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    WindowId::from(uuid::Uuid::from_bytes(bytes))
}

fn generate_control_token() -> Result<String, ApiError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| {
        ApiError::new(
            ErrorCode::Internal,
            format!("generate control token: {error}"),
        )
        .retryable()
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn valid_tmux_window_id(value: &str) -> bool {
    value
        .strip_prefix('@')
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
}

fn backend(context: &str, error: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorCode::Backend, format!("{context}: {error}")).retryable()
}
