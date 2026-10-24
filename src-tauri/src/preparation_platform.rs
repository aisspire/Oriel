//! Native process ownership and file selection. No business-tool knowledge.
use oriel_core::{preparation::OwnedProcess, Result};
use std::process::Command;

#[cfg(not(windows))]
pub fn launch(_: &mut Command) -> Result<OwnedProcess> {
    Err("网页前置程序目前仅支持 Windows 桌面。".into())
}
#[cfg(not(windows))]
pub fn pick_file(_owner: usize) -> Result<Option<String>> {
    Err("文件选择目前仅支持 Windows。".into())
}

#[cfg(windows)]
mod native {
    use super::*;
    use oriel_core::preparation::ProcessTree;
    use std::{
        mem::size_of,
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::Child,
        time::{Duration, Instant},
    };
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{CloseHandle, HANDLE, HWND},
            System::{
                Com::{
                    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
                },
                Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                    THREADENTRY32,
                },
                JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW,
                    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
                    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
                    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                },
                Threading::{
                    OpenThread, ResumeThread, CREATE_NO_WINDOW, CREATE_SUSPENDED,
                    THREAD_SUSPEND_RESUME,
                },
            },
            UI::Shell::{
                FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
                SIGDN_FILESYSPATH,
            },
        },
    };
    struct Handle(HANDLE);
    unsafe impl Send for Handle {}
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    struct Job(Handle);
    impl ProcessTree for Job {
        fn stop(&mut self, child: &mut Child) -> Result<()> {
            unsafe {
                TerminateJobObject(self.0 .0, 1).map_err(|e| e.to_string())?;
            }
            let deadline = Instant::now() + Duration::from_secs(9);
            loop {
                let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                unsafe {
                    QueryInformationJobObject(
                        Some(self.0 .0),
                        JobObjectBasicAccountingInformation,
                        &mut accounting as *mut _ as *mut _,
                        size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                        None,
                    )
                    .map_err(|e| e.to_string())?;
                }
                if accounting.ActiveProcesses == 0 {
                    let _ = child.wait();
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err("Windows 未确认所属进程树退出。".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    fn resume(child: &Child) -> Result<()> {
        unsafe {
            let snapshot =
                Handle(CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(|e| e.to_string())?);
            let mut entry = THREADENTRY32 {
                dwSize: size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            Thread32First(snapshot.0, &mut entry).map_err(|e| e.to_string())?;
            loop {
                if entry.th32OwnerProcessID == child.id() {
                    let thread = Handle(
                        OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                            .map_err(|e| e.to_string())?,
                    );
                    if ResumeThread(thread.0) == u32::MAX {
                        return Err("无法恢复前置程序线程。".into());
                    }
                    return Ok(());
                }
                if Thread32Next(snapshot.0, &mut entry).is_err() {
                    break;
                }
            }
        }
        Err("找不到挂起程序的主线程。".into())
    }
    pub fn launch(command: &mut Command) -> Result<OwnedProcess> {
        let job = unsafe {
            let handle = Handle(CreateJobObjectW(None, PCWSTR::null()).map_err(|e| e.to_string())?);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|e| e.to_string())?;
            Job(handle)
        };
        command.creation_flags((CREATE_SUSPENDED | CREATE_NO_WINDOW).0);
        let mut child = command
            .spawn()
            .map_err(|e| format!("无法启动前置程序：{e}"))?;
        let attach = unsafe {
            AssignProcessToJobObject(job.0 .0, HANDLE(child.as_raw_handle()))
                .map_err(|e| e.to_string())
        }
        .and_then(|_| resume(&child));
        if let Err(error) = attach {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("无法托管前置程序：{error}"));
        }
        Ok(OwnedProcess {
            child,
            tree: Box::new(job),
        })
    }
    pub fn pick_file(owner: usize) -> Result<Option<String>> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
            struct Com;
            impl Drop for Com {
                fn drop(&mut self) {
                    unsafe {
                        CoUninitialize();
                    }
                }
            }
            let _com = Com;
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| e.to_string())?;
            dialog
                .SetOptions(FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)
                .map_err(|e| e.to_string())?;
            if let Err(error) = dialog.Show(Some(HWND(owner as *mut _))) {
                if error.code().0 as u32 == 0x800704c7 {
                    return Ok(None);
                }
                return Err(error.to_string());
            }
            let item = dialog.GetResult().map_err(|e| e.to_string())?;
            let path = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|e| e.to_string())?;
            let value = path.to_string().map_err(|e| e.to_string());
            CoTaskMemFree(Some(path.0 as *const _));
            value.map(Some)
        }
    }
}
#[cfg(windows)]
pub use native::launch;
#[cfg(windows)]
pub fn pick_file(owner: usize) -> Result<Option<String>> {
    std::thread::spawn(move || native::pick_file(owner))
        .join()
        .map_err(|_| "文件选择线程异常退出。".to_owned())?
}
