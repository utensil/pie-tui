//! JS event-loop bridge for the editor autocomplete provider.
//!
//! The component editor remains the owner of autocomplete snapshots,
//! cancellation, stale-result checks, and menu state. This module provides a
//! small queued host for the Node adapter. JavaScript drains the host actions,
//! runs provider promises after the native call has returned, and settles each
//! request back into the native state.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread::ThreadId;

use napi::bindgen_prelude::{Env, FnArgs, FunctionRef, JsObjectValue, Object};
use napi_derive::napi;
use pie_components::{
    AutocompleteItem, AutocompleteOptions, AutocompleteProvider, AutocompleteSuggestions,
    CompletionResult, EditorAutocompleteFuture, EditorHost, EditorHostTask, EditorTaskId,
};

/// Data passed to the JS event loop for one provider request.
#[napi(object)]
pub struct NativeAutocompleteRequest {
    pub key: u32,
    pub provider_id: u32,
    pub lines: Vec<String>,
    pub cursor_line: u32,
    pub cursor_col: u32,
    pub force: bool,
}

/// A queued host action. Optional fields keep the native ABI compact; `kind`
/// determines which fields are populated.
#[napi(object)]
pub struct NativeAutocompleteAction {
    pub kind: String,
    pub task_id: Option<u32>,
    pub delay_ms: Option<u32>,
    pub key: Option<u32>,
    pub request: Option<NativeAutocompleteRequest>,
}

#[napi(object)]
pub struct NativeAutocompleteItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

#[napi(object)]
pub struct NativeAutocompleteSuggestions {
    pub items: Vec<NativeAutocompleteItem>,
    pub prefix: String,
}

#[napi(object)]
pub struct NativeCompletionResult {
    pub lines: Vec<String>,
    pub cursor_line: u32,
    pub cursor_col: u32,
}

type ApplyArguments = FnArgs<(Vec<String>, u32, u32, NativeAutocompleteItem, String)>;
type ShouldArguments = FnArgs<(Vec<String>, u32, u32)>;
type ApplyFunctionRef = FunctionRef<ApplyArguments, NativeCompletionResult>;
type ShouldFunctionRef = FunctionRef<ShouldArguments, bool>;

enum HostAction {
    Schedule {
        task_id: EditorTaskId,
        delay_ms: u64,
    },
    Cancel {
        task_id: EditorTaskId,
    },
    Request(NativeAutocompleteRequest),
    Abort {
        key: u32,
    },
    Render,
}

struct ResultSlot {
    result: Option<Option<AutocompleteSuggestions>>,
}

#[derive(Default)]
pub(crate) struct BridgeState {
    next_task_id: u32,
    next_request_key: u32,
    tasks: HashMap<EditorTaskId, EditorHostTask>,
    futures: HashMap<u64, EditorAutocompleteFuture>,
    result_slots: HashMap<u32, ResultSlot>,
    actions: Vec<HostAction>,
}

/// Shared queued host state. The mutex is held only while moving facts in or
/// out of the queue; editor callbacks and future polling happen after it is
/// released so they can enqueue follow-up work safely.
pub type SharedBridge = Arc<Mutex<BridgeState>>;

pub fn new_bridge() -> SharedBridge {
    Arc::new(Mutex::new(BridgeState::default()))
}

pub struct QueuedEditorHost {
    bridge: SharedBridge,
    rows: usize,
}

impl QueuedEditorHost {
    pub fn new(bridge: SharedBridge, rows: usize) -> Self {
        Self { bridge, rows }
    }

    pub fn shared_bridge(&self) -> SharedBridge {
        Arc::clone(&self.bridge)
    }

    fn next_request_key(&self) -> u32 {
        let mut bridge = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned");
        loop {
            bridge.next_request_key = bridge.next_request_key.wrapping_add(1).max(1);
            if !bridge.result_slots.contains_key(&bridge.next_request_key) {
                return bridge.next_request_key;
            }
        }
    }
}

impl EditorHost for QueuedEditorHost {
    fn terminal_rows(&self) -> usize {
        self.rows
    }

    fn request_render(&mut self, _force: bool) {
        self.bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .actions
            .push(HostAction::Render);
    }

    fn schedule_task(&mut self, delay_ms: u64, task: EditorHostTask) -> EditorTaskId {
        let mut bridge = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned");
        loop {
            bridge.next_task_id = bridge.next_task_id.wrapping_add(1).max(1);
            if !bridge
                .tasks
                .contains_key(&EditorTaskId(u64::from(bridge.next_task_id)))
            {
                break;
            }
        }
        let task_id = EditorTaskId(u64::from(bridge.next_task_id));
        bridge.tasks.insert(task_id, task);
        bridge
            .actions
            .push(HostAction::Schedule { task_id, delay_ms });
        task_id
    }

    fn cancel_task(&mut self, task_id: EditorTaskId) {
        let mut bridge = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned");
        bridge.tasks.remove(&task_id);
        bridge.actions.push(HostAction::Cancel { task_id });
    }

    fn spawn_autocomplete(&mut self, request_id: u64, future: EditorAutocompleteFuture) {
        self.bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .futures
            .insert(request_id, future);
    }

    fn discard_autocomplete(&mut self, request_id: u64) {
        let future = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .futures
            .remove(&request_id);
        drop(future);
    }
}

impl QueuedEditorHost {
    pub fn take_task(&self, task_id: EditorTaskId) -> Option<EditorHostTask> {
        self.bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .tasks
            .remove(&task_id)
    }

    fn take_futures(&self) -> Vec<(u64, EditorAutocompleteFuture)> {
        let mut bridge = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned");
        bridge.futures.drain().collect()
    }

    fn put_futures(&self, futures: Vec<(u64, EditorAutocompleteFuture)>) {
        let mut bridge = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned");
        for (request_id, future) in futures {
            bridge.futures.insert(request_id, future);
        }
    }

    pub fn settle(&self, key: u32, result: Option<AutocompleteSuggestions>) -> bool {
        let mut bridge = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned");
        let Some(slot) = bridge.result_slots.get_mut(&key) else {
            return false;
        };
        slot.result = Some(result);
        true
    }

    pub fn take_actions(&self) -> Vec<NativeAutocompleteAction> {
        let actions = std::mem::take(
            &mut self
                .bridge
                .lock()
                .expect("autocomplete bridge mutex poisoned")
                .actions,
        );
        actions
            .into_iter()
            .map(|action| match action {
                HostAction::Schedule { task_id, delay_ms } => NativeAutocompleteAction {
                    kind: "schedule".to_owned(),
                    task_id: Some(task_id.0 as u32),
                    delay_ms: Some(delay_ms.min(u64::from(u32::MAX)) as u32),
                    key: None,
                    request: None,
                },
                HostAction::Cancel { task_id } => NativeAutocompleteAction {
                    kind: "cancel".to_owned(),
                    task_id: Some(task_id.0 as u32),
                    delay_ms: None,
                    key: None,
                    request: None,
                },
                HostAction::Request(request) => NativeAutocompleteAction {
                    kind: "request".to_owned(),
                    task_id: None,
                    delay_ms: None,
                    key: Some(request.key),
                    request: Some(request),
                },
                HostAction::Abort { key } => NativeAutocompleteAction {
                    kind: "abort".to_owned(),
                    task_id: None,
                    delay_ms: None,
                    key: Some(key),
                    request: None,
                },
                HostAction::Render => NativeAutocompleteAction {
                    kind: "render".to_owned(),
                    task_id: None,
                    delay_ms: None,
                    key: None,
                    request: None,
                },
            })
            .collect()
    }

    /// Poll all currently pending provider futures. The bridge mutex is never
    /// held while a future is polled or while the editor receives a result.
    pub fn pump(&self, editor: &mut pie_components::Editor) {
        loop {
            let pending = self.take_futures();
            if pending.is_empty() {
                return;
            }
            let waker = Waker::noop();
            let mut context = Context::from_waker(waker);
            let mut waiting = Vec::with_capacity(pending.len());
            let mut ready = Vec::new();
            for (request_id, mut future) in pending {
                match future.as_mut().poll(&mut context) {
                    Poll::Pending => waiting.push((request_id, future)),
                    Poll::Ready(result) => ready.push((request_id, result)),
                }
            }
            self.put_futures(waiting);
            if ready.is_empty() {
                return;
            }
            for (request_id, result) in ready {
                editor.complete_autocomplete(request_id, result);
            }
        }
    }

    fn insert_result_slot(&self, key: u32) {
        self.bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .result_slots
            .insert(key, ResultSlot { result: None });
    }

    fn take_result_slot(&self, key: u32) -> Option<Option<AutocompleteSuggestions>> {
        let mut bridge = self
            .bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned");
        let slot = bridge.result_slots.get_mut(&key)?;
        let result = slot.result.take()?;
        bridge.result_slots.remove(&key);
        Some(result)
    }

    fn queue_request(&self, request: NativeAutocompleteRequest) {
        self.insert_result_slot(request.key);
        self.bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .actions
            .push(HostAction::Request(request));
    }

    fn queue_abort(&self, key: u32) {
        self.bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .actions
            .push(HostAction::Abort { key });
    }
}

/// JS-backed provider. Its async callback is represented as a Send future that
/// queues a request on first poll and waits for `settle_autocomplete` to fill
/// the result slot. The async getSuggestions hook runs after the native call
/// returns; sync hooks use the owning environment scope and native borrow guard.
pub struct JsAutocompleteProvider {
    bridge: SharedBridge,
    trigger_characters: Vec<String>,
    apply_completion: ApplyFunctionRef,
    should_trigger_file_completion: Option<ShouldFunctionRef>,
    owner_thread: ThreadId,
    owner_env: usize,
    provider_id: u32,
}

pub fn provider_from_js(
    env: Env,
    provider: Object<'_>,
    bridge: SharedBridge,
) -> napi::Result<Arc<dyn AutocompleteProvider>> {
    let trigger_characters = provider
        .get_named_property::<Option<Vec<String>>>("triggerCharacters")?
        .unwrap_or_default();
    let apply_completion = provider.get_named_property::<ApplyFunctionRef>("applyCompletion")?;
    let should_trigger_file_completion =
        provider.get_named_property::<Option<ShouldFunctionRef>>("shouldTriggerFileCompletion")?;
    Ok(Arc::new(JsAutocompleteProvider {
        bridge,
        trigger_characters,
        apply_completion,
        should_trigger_file_completion,
        owner_thread: std::thread::current().id(),
        owner_env: env.raw() as usize,
        provider_id: provider.get_named_property("providerId")?,
    }))
}

impl AutocompleteProvider for JsAutocompleteProvider {
    fn trigger_characters(&self) -> Option<&[String]> {
        Some(&self.trigger_characters)
    }

    fn get_suggestions<'a>(
        &'a self,
        lines: &'a [String],
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> EditorAutocompleteFuture {
        let bridge = Arc::clone(&self.bridge);
        let key = QueuedEditorHost::new(Arc::clone(&bridge), 0).next_request_key();
        let request = NativeAutocompleteRequest {
            key,
            provider_id: self.provider_id,
            lines: lines.to_vec(),
            cursor_line: cursor_line.min(u32::MAX as usize) as u32,
            cursor_col: cursor_col.min(u32::MAX as usize) as u32,
            force: options.force,
        };
        let abort_bridge = Arc::clone(&bridge);
        options.signal.on_cancel(move || {
            QueuedEditorHost::new(Arc::clone(&abort_bridge), 0).queue_abort(key);
        });
        Box::pin(JsAutocompleteFuture {
            bridge,
            key,
            request: Some(request),
        })
    }

    fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult {
        self.try_apply_completion(lines, cursor_line, cursor_col, item, prefix)
            .unwrap_or_else(|| identity_completion(lines, cursor_line, cursor_col))
    }

    fn try_apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> Option<CompletionResult> {
        let args = FnArgs::from((
            lines.to_vec(),
            cursor_line.min(u32::MAX as usize) as u32,
            cursor_col.min(u32::MAX as usize) as u32,
            NativeAutocompleteItem::from(item),
            prefix.to_owned(),
        ));
        let result = with_current_env(self.owner_thread, self.owner_env, |env| {
            self.apply_completion
                .borrow_back(env)
                .and_then(|function| function.call(args))
        });
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                record_callback_error(error);
                return None;
            }
        };
        Some(CompletionResult {
            lines: result.lines,
            cursor_line: result.cursor_line as usize,
            cursor_col: result.cursor_col as usize,
        })
    }

    fn should_trigger_file_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
    ) -> bool {
        let Some(function_ref) = &self.should_trigger_file_completion else {
            return true;
        };
        let result = with_current_env(self.owner_thread, self.owner_env, |env| {
            function_ref.borrow_back(env).and_then(|function| {
                function.call(FnArgs::from((
                    lines.to_vec(),
                    cursor_line.min(u32::MAX as usize) as u32,
                    cursor_col.min(u32::MAX as usize) as u32,
                )))
            })
        });
        match result {
            Ok(result) => result,
            Err(error) => {
                record_callback_error(error);
                false
            }
        }
    }
}

struct JsAutocompleteFuture {
    bridge: SharedBridge,
    key: u32,
    request: Option<NativeAutocompleteRequest>,
}

impl Future for JsAutocompleteFuture {
    type Output = Option<AutocompleteSuggestions>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        if let Some(request) = this.request.take() {
            QueuedEditorHost::new(Arc::clone(&this.bridge), 0).queue_request(request);
            return Poll::Pending;
        }
        if let Some(result) =
            QueuedEditorHost::new(Arc::clone(&this.bridge), 0).take_result_slot(this.key)
        {
            return Poll::Ready(result);
        }
        // Cancellation is delivered to JavaScript as an AbortController
        // action. The future remains pending until that provider promise
        // settles, preserving the editor's queued-request serialization.
        Poll::Pending
    }
}

impl Drop for JsAutocompleteFuture {
    fn drop(&mut self) {
        self.bridge
            .lock()
            .expect("autocomplete bridge mutex poisoned")
            .result_slots
            .remove(&self.key);
    }
}

fn identity_completion(
    lines: &[String],
    cursor_line: usize,
    cursor_col: usize,
) -> CompletionResult {
    CompletionResult {
        lines: lines.to_vec(),
        cursor_line,
        cursor_col,
    }
}

impl From<&AutocompleteItem> for NativeAutocompleteItem {
    fn from(value: &AutocompleteItem) -> Self {
        Self {
            value: value.value.clone(),
            label: value.label.clone(),
            description: value.description.clone(),
        }
    }
}

impl From<NativeAutocompleteItem> for AutocompleteItem {
    fn from(value: NativeAutocompleteItem) -> Self {
        Self {
            value: value.value,
            label: value.label,
            description: value.description,
        }
    }
}

impl From<NativeAutocompleteSuggestions> for AutocompleteSuggestions {
    fn from(value: NativeAutocompleteSuggestions) -> Self {
        Self {
            items: value.items.into_iter().map(Into::into).collect(),
            prefix: value.prefix,
        }
    }
}

struct CallbackContext {
    env: Env,
    error: Option<napi::Error>,
}

thread_local! {
    static CURRENT_CONTEXT: std::cell::RefCell<Option<CallbackContext>> = const { std::cell::RefCell::new(None) };
}

/// Scope the N-API environment and callback errors to one native entrypoint.
/// Nested calls to other editors restore the outer scope when they return.
/// NAPI's NativeBorrowScope rejects reentry into the currently borrowed editor.
pub struct CurrentEnvScope {
    previous: Option<CallbackContext>,
}

impl CurrentEnvScope {
    pub fn enter(env: Env) -> Self {
        let previous =
            CURRENT_CONTEXT.with(|slot| slot.replace(Some(CallbackContext { env, error: None })));
        Self { previous }
    }

    pub fn finish<T>(&self, value: T) -> napi::Result<T> {
        CURRENT_CONTEXT.with(|slot| {
            match slot
                .borrow_mut()
                .as_mut()
                .and_then(|context| context.error.take())
            {
                Some(error) => Err(error),
                None => Ok(value),
            }
        })
    }
}

impl Drop for CurrentEnvScope {
    fn drop(&mut self) {
        CURRENT_CONTEXT.with(|slot| slot.replace(self.previous.take()));
    }
}

fn record_callback_error(error: napi::Error) {
    CURRENT_CONTEXT.with(|slot| {
        if let Some(context) = slot.borrow_mut().as_mut() {
            context.error.get_or_insert(error);
        }
    });
}

fn with_current_env<T>(
    owner_thread: ThreadId,
    owner_env: usize,
    callback: impl FnOnce(&Env) -> napi::Result<T>,
) -> napi::Result<T> {
    let env = CURRENT_CONTEXT.with(|slot| slot.borrow().as_ref().map(|context| context.env));
    match env {
        Some(env)
            if owner_thread == std::thread::current().id() && owner_env == env.raw() as usize =>
        {
            callback(&env)
        }
        _ => Err(napi::Error::from_reason(
            "Autocomplete callback used outside its owning JavaScript environment",
        )),
    }
}
