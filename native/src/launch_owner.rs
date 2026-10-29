use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use neon::prelude::*;
use winapi::um::processthreadsapi::TerminateProcess;
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::{WAIT_FAILED, WAIT_OBJECT_0};

use crate::Process;

#[derive(Default)]
struct State {
    cancelled: bool,
    process: Option<Arc<Process>>,
}

/// Owns one launch, including the interval before CreateProcess returns its handle.
#[derive(Default)]
pub struct LaunchOwner(Mutex<State>);

impl LaunchOwner {
    pub fn check(&self) -> Result<()> {
        if self.0.lock().unwrap().cancelled {
            Err(anyhow!("Process launch cancelled"))
        } else {
            Ok(())
        }
    }

    pub fn bind(&self, process: Arc<Process>) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.cancelled {
            // The caller's termination guard still owns this suspended process. It must never
            // resume after cancellation, even if CreateProcess completed after the caller stopped.
            return Err(anyhow!("Process launch cancelled"));
        }
        if state.process.is_some() {
            return Err(anyhow!("Launch owner already has a process"));
        }
        state.process = Some(process);
        Ok(())
    }

    fn cancel(&self) -> Option<Arc<Process>> {
        let mut state = self.0.lock().unwrap();
        state.cancelled = true;
        state.process.clone()
    }
}

impl Finalize for LaunchOwner {}

pub fn terminate_and_wait(process: &Process) -> Result<()> {
    unsafe {
        let handle = process.handle();
        if WaitForSingleObject(handle, 0) == WAIT_OBJECT_0 {
            return Ok(());
        }
        // The handle came directly from this launch's CreateProcess call. Never reopen by PID.
        if TerminateProcess(handle, 1223) == 0 && WaitForSingleObject(handle, 0) != WAIT_OBJECT_0 {
            return Err(anyhow!(
                "Terminating owned process failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        match WaitForSingleObject(handle, 5000) {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_FAILED => Err(anyhow!(
                "Waiting for owned process failed: {}",
                std::io::Error::last_os_error()
            )),
            _ => Err(anyhow!(
                "Owned process termination was not confirmed within 5 seconds"
            )),
        }
    }
}

pub fn create_launch_owner(mut cx: FunctionContext) -> JsResult<JsBox<Arc<LaunchOwner>>> {
    Ok(cx.boxed(Arc::new(LaunchOwner::default())))
}

pub fn terminate_launch_owner(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let owner = cx.argument::<JsBox<Arc<LaunchOwner>>>(0)?;
    // Mark cancellation before returning to JS; a pending launch cannot bind/resume afterward.
    let process = owner.cancel();
    let channel = cx.channel();
    let (deferred, promise) = cx.promise();
    std::thread::spawn(move || {
        let result = process.map_or(Ok(()), |p| terminate_and_wait(&p));
        let _ = deferred.try_settle_with(&channel, move |mut cx| match result {
            Ok(()) => Ok(cx.undefined()),
            Err(err) => cx.throw_error(format!("{err:#}")),
        });
    });
    Ok(promise)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerminateOnDrop;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::ResumeThread;

    fn suspended_fixture() -> (Arc<Process>, winapi::um::winnt::HANDLE) {
        let exe = std::env::current_exe().unwrap();
        let exe = exe.to_str().unwrap();
        let args = format!("\"{exe}\" --ignored --exact launch_owner::tests::hung_fixture");
        let (process, thread) = Process::new_from_path(exe, &args, ".").unwrap();
        (Arc::new(process), thread)
    }

    #[test]
    #[ignore]
    fn hung_fixture() {
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn termination_uses_only_owned_handle_and_confirms_exit() {
        let (owned, thread) = suspended_fixture();
        let _owned_guard = TerminateOnDrop(&owned);
        let (unrelated, other_thread) = suspended_fixture();
        let _unrelated_guard = TerminateOnDrop(&unrelated);
        unsafe {
            ResumeThread(thread);
            CloseHandle(thread);
            ResumeThread(other_thread);
            CloseHandle(other_thread);
        }
        let owner = LaunchOwner::default();
        owner.bind(Arc::clone(&owned)).unwrap();
        terminate_and_wait(&owner.cancel().unwrap()).unwrap();
        terminate_and_wait(&owner.cancel().unwrap()).unwrap();
        assert!(owner.check().is_err());
        assert_eq!(
            unsafe { WaitForSingleObject(owned.handle(), 0) },
            WAIT_OBJECT_0
        );
        assert_ne!(
            unsafe { WaitForSingleObject(unrelated.handle(), 0) },
            WAIT_OBJECT_0
        );
        terminate_and_wait(&unrelated).unwrap();
    }

    #[test]
    fn cancellation_rejects_a_late_suspended_process() {
        let owner = LaunchOwner::default();
        assert!(owner.cancel().is_none());
        assert!(owner.check().is_err());
        let (process, thread) = suspended_fixture();
        {
            let _guard = TerminateOnDrop(&process);
            assert!(owner.bind(Arc::clone(&process)).is_err());
        }
        process.wait(5000);
        assert_eq!(
            unsafe { WaitForSingleObject(process.handle(), 0) },
            WAIT_OBJECT_0
        );
        unsafe {
            CloseHandle(thread);
        }
    }
}
