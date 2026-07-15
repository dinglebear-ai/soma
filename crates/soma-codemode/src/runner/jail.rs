pub(super) fn reset_execution_jail() {
    let Some(base) = jail_base_path() else {
        return;
    };
    EXECUTION_JAIL.with(|cell| {
        let mut cell = cell.borrow_mut();
        if let Some(previous) = cell.take() {
            let _ = std::env::set_current_dir(&base);
            let _ = std::fs::remove_dir_all(previous);
        }
        let jail = base.join(format!("exec-{}-{}", std::process::id(), next_jail_seq()));
        if std::fs::create_dir(&jail).is_ok() && std::env::set_current_dir(&jail).is_ok() {
            *cell = Some(jail);
        } else {
            let _ = std::fs::remove_dir_all(&jail);
            let _ = std::env::set_current_dir(&base);
        }
    });
}

pub(super) fn cleanup_execution_jail(drop_base: bool) {
    let base = JAIL_BASE.with(|cell| cell.borrow().as_ref().map(|dir| dir.path().to_path_buf()));
    EXECUTION_JAIL.with(|cell| {
        if let Some(jail) = cell.borrow_mut().take() {
            if let Some(base) = &base {
                let _ = std::env::set_current_dir(base);
            }
            let _ = std::fs::remove_dir_all(jail);
        }
    });
    if drop_base {
        let _ = std::env::set_current_dir(std::env::temp_dir());
        JAIL_BASE.with(|cell| {
            drop(cell.borrow_mut().take());
        });
    }
}

fn jail_base_path() -> Option<std::path::PathBuf> {
    JAIL_BASE.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = tempfile::Builder::new()
                .prefix("soma-codemode-")
                .tempdir()
                .ok();
        }
        cell.as_ref().map(|dir| dir.path().to_path_buf())
    })
}

fn next_jail_seq() -> u64 {
    JAIL_SEQ.with(|seq| {
        let next = seq.get();
        seq.set(next.saturating_add(1));
        next
    })
}

thread_local! {
    static JAIL_BASE: std::cell::RefCell<Option<tempfile::TempDir>> =
        const { std::cell::RefCell::new(None) };
    static EXECUTION_JAIL: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
    static JAIL_SEQ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
