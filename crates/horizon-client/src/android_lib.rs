//! Horizon on Android (Starling): the cdylib's entry points.
//!
//! The crate root of horizon-android (crates/horizon-android/Cargo.toml). Android and the JVM look
//! these up by name in the library, so they live here, in the library that is linked last; each
//! one calls `horizon_client::android`, where the game's Android hooks and its start-up are.
#![cfg(target_os = "android")]

use winit::platform::android::activity::AndroidApp;

/// NativeActivity's entry point (the manifest's `android.app.func_name`): the game of this
/// library, or of the build with native mods StarlingActivity loaded in its place.
///
/// # Safety
///
/// NativeActivity's call: an `ANativeActivity` and its saved state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn starling_on_create(
    activity: *mut std::ffi::c_void,
    saved: *mut std::ffi::c_void,
    saved_size: usize,
) {
    // SAFETY: NativeActivity's own arguments, passed on.
    unsafe { horizon_client::android::on_create(activity, saved, saved_size) }
}

/// android-activity's entry point calls it on the game's thread.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    horizon_client::android::main(app);
}

/// `CallbackBridge.getFps`: the fps the launcher's counter shows. The game keeps none of its own,
/// so the loop counts the frames it draws and this measures them.
#[unsafe(no_mangle)]
extern "system" fn Java_org_lwjgl_glfw_CallbackBridge_getFps(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jint {
    horizon_client::android::fps()
}

/// `StarlingActivity.startEngine`: the launcher's Activity is not a NativeActivity, so the engine
/// gets its thread here instead. The activity, its APK's assets and its files directory are kept
/// for the JNI calls that follow; the thread runs until the game ends.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_startEngine(
    mut env: jni::JNIEnv,
    // An instance method: the second argument is the activity itself, not its class.
    activity: jni::objects::JObject,
    files: jni::objects::JString,
    assets: jni::objects::JObject,
) {
    let Ok(vm) = env.get_java_vm() else {
        return;
    };
    let Ok(activity) = env.new_global_ref(activity) else {
        return;
    };
    let files_dir: String = match env.get_string(&files) {
        Ok(text) => String::from(text),
        Err(_) => return,
    };
    let files = std::path::PathBuf::from(&files_dir);
    // The trace follows the game's own files: /data/local/tmp is not the app's to write.
    // SAFETY: the AssetManager is the activity's and lives as long as it does; fromJava returns
    // the manager the object wraps, not a copy this owns.
    unsafe extern "C" {
        fn AAssetManager_fromJava(
            env: *mut jni::sys::JNIEnv,
            manager: jni::sys::jobject,
        ) -> *mut std::ffi::c_void;
    }
    let assets = unsafe { AAssetManager_fromJava(env.get_raw(), assets.as_raw()) };
    // SAFETY: the VM outlives the process; the activity reference and the manager are the ones
    // just taken, kept for the game's whole run.
    unsafe {
        horizon_client::android::attach_launcher(vm.get_java_vm_pointer(), activity, files, assets)
    };
    // logcat's buffer rolls over long before a launcher's start is looked at: the steps go to a
    // file the launcher can read back.
    let trace = |line: &str| {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{files_dir}/engine_trace.txt"))
        {
            let _ = writeln!(file, "{line}");
        }
        horizon_client::android::logcat_line(false, line);
    };
    trace(&format!(
        "startEngine: activity attached, files {files_dir}"
    ));
    match std::thread::Builder::new().name("render".into()).spawn(|| {
        // A phone has no terminal for a panic to scroll past: the message goes to the
        // launcher instead of ending the process with SIGABRT.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = format!("engine stopped: {info}");
            horizon_client::android::trace(&message);
            horizon_client::android::report_failure(&message);
            hook(info);
        }));
        if let Err(panic) = std::panic::catch_unwind(horizon_client::android::launcher_main) {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "the engine panicked".into());
            let message = format!("engine stopped: {message}");
            horizon_client::android::trace(&message);
            horizon_client::android::report_failure(&message);
        }
    }) {
        Ok(_) => trace("startEngine: thread up"),
        Err(e) => trace(&format!("startEngine: the thread did not start: {e}")),
    }
}

/// `StarlingActivity.onStorage`, called on the UI thread before the game's thread starts.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_onStorage(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    folder: jni::objects::JString,
) {
    if folder.is_null() {
        return;
    }
    if let Ok(folder) = env.get_string(&folder) {
        horizon_client::android::set_shared_folder(String::from(folder).into());
    }
}

/// `StarlingActivity.onImeHeight` (packaging/android/java), called on the UI thread.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_onImeHeight(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    pixels: i32,
) {
    horizon_client::android::set_ime_height(pixels);
}

/// `TextMirror.onMirrorEdit`, called on the UI thread.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_TextMirror_onMirrorEdit(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    text: jni::objects::JString,
    cursor: i32,
    anchor: i32,
    generation: i32,
) {
    let Ok(text) = env.get_string(&text) else {
        return;
    };
    horizon_client::android::mirror_edit(String::from(text), cursor, anchor, generation);
}

/// `TextMirror.onMirrorAction`, called on the UI thread.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_TextMirror_onMirrorAction(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    horizon_client::android::mirror_action();
}

/// `TextMirror.onMirrorKey`, called on the UI thread.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_TextMirror_onMirrorKey(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    code: i32,
) {
    horizon_client::android::mirror_key(code);
}

/// `StarlingActivity.onPointerCaptureLost`, called on the UI thread.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_onPointerCaptureLost(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    horizon_client::android::note_pointer_capture_lost();
}

/// `StarlingActivity.setSurface`: the launcher's SurfaceView, which the engine draws into instead
/// of the activity's own window. `surface` is an android.view.Surface; null clears it and the
/// activity's window is used again.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_setSurface(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    surface: jni::objects::JObject,
) {
    // NDK's own: libandroid.so is already loaded by the activity, and its prototypes are stable.
    unsafe extern "C" {
        fn ANativeWindow_fromSurface(
            env: *mut jni::sys::JNIEnv,
            surface: jni::sys::jobject,
        ) -> *mut std::ffi::c_void;
        fn ANativeWindow_release(window: *mut std::ffi::c_void);
    }

    // Each call takes a reference, so the previous one goes: a SurfaceView that recreates its
    // surface (a resize, the activity coming back) would otherwise leak one window per call.
    let previous = horizon_client::android::external_window();
    if !surface.is_null() {
        let raw = env.get_raw();
        // SAFETY: a live Surface and the process's JNIEnv, both owned by the caller's thread.
        let window = unsafe { ANativeWindow_fromSurface(raw, surface.as_raw()) };
        if !window.is_null() {
            horizon_client::android::set_external_window(window as usize);
            // SAFETY: a reference this file took and no longer keeps.
            if let Some(old) = previous {
                unsafe { ANativeWindow_release(old as *mut std::ffi::c_void) };
            }
            return;
        }
    }
    horizon_client::android::set_external_window(0);
    if let Some(old) = previous {
        // SAFETY: as above.
        unsafe { ANativeWindow_release(old as *mut std::ffi::c_void) };
    }
}

/// `StarlingActivity.onMinecraftRoot`: where the launcher's own Minecraft install is (FCL's
/// `.minecraft`, or one of its version folders), when it has one of its own instead of the
/// engine's `minecraft/` under the shared folder.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_onMinecraftRoot(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    folder: jni::objects::JString,
) {
    if folder.is_null() {
        return;
    }
    if let Ok(folder) = env.get_string(&folder) {
        horizon_client::android::set_minecraft_root(String::from(folder).into());
    }
}

/// The account the launcher signed in with: `StarlingActivity.setAccount`. The launcher has the
/// consent screen and the browser; the engine keeps the tokens sealed by the device's keystore
/// and plays as that account, which a copied `accounts.json` alone cannot do (it holds names and
/// UUIDs, no credentials).
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_setAccount(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    uuid: jni::objects::JString,
    name: jni::objects::JString,
    provider: jni::objects::JString,
    refresh_token: jni::objects::JString,
    minecraft_token: jni::objects::JString,
    expires_at: jni::sys::jlong,
    xuid: jni::objects::JString,
    client_id: jni::objects::JString,
) {
    let read = |env: &mut jni::JNIEnv, s: &jni::objects::JString| -> String {
        if s.is_null() {
            return String::new();
        }
        env.get_string(s).map(String::from).unwrap_or_default()
    };
    let uuid = read(&mut env, &uuid);
    let name = read(&mut env, &name);
    let provider = read(&mut env, &provider);
    let refresh = read(&mut env, &refresh_token);
    let token = read(&mut env, &minecraft_token);
    let xuid = read(&mut env, &xuid);
    let client_id = read(&mut env, &client_id);
    let provider = if provider.eq_ignore_ascii_case("Ely") {
        horizon_client::accounts::Provider::Ely
    } else {
        horizon_client::accounts::Provider::Microsoft
    };
    let result = horizon_client::accounts::accept_account(
        &uuid,
        &name,
        provider,
        &refresh,
        &token,
        expires_at.max(0) as u64,
        (!xuid.is_empty()).then_some(xuid.as_str()),
        &client_id,
    );
    // Handed back so the launcher can put it in its trace: a failure here is why the game
    // plays offline, and stderr never reaches a file a report would carry.
    let reply = match result {
        Ok(()) => String::new(),
        Err(e) => e,
    };
    let _ = env.new_string(reply).map(|s| s.into_raw());
}

/// The launcher's on-screen controls: `StarlingActivity.sendMouseMove/Button/Wheel`
/// (packaging/android/java). They feed the same queue winit's own mouse events go to
/// (android::input), so the game reads them as a computer's mouse.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_sendMouseMove(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
) {
    horizon_client::android::push_mouse(horizon_client::android::MouseEvent::Moved(
        f64::from(x),
        f64::from(y),
    ));
}

/// `button`: 1 left, 2 right, 3 middle (the engine's own numbering, see `android::input`).
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_sendMouseButton(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    button: jni::sys::jint,
    down: jni::sys::jboolean,
) {
    let button = match button {
        1 => winit::event::MouseButton::Left,
        2 => winit::event::MouseButton::Right,
        _ => winit::event::MouseButton::Middle,
    };
    horizon_client::android::push_mouse(horizon_client::android::MouseEvent::Button(
        button,
        down != 0,
    ));
}

#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_sendMouseWheel(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
) {
    horizon_client::android::push_mouse(horizon_client::android::MouseEvent::Wheel(x, y));
}

/// How far the pointer went while the launcher's own control drove it: the view turns by it.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_sendMouseMotion(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    dx: jni::sys::jfloat,
    dy: jni::sys::jfloat,
) {
    horizon_client::android::push_mouse(horizon_client::android::MouseEvent::Motion(
        f64::from(dx),
        f64::from(dy),
    ));
}

/// `name` is a `winit::keyboard::KeyCode` variant name ("KeyW", "Space"): the launcher's
/// controls name the key they press, and the engine looks the name up.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_sendKey(
    mut env: jni::JNIEnv,
    _class: *mut std::ffi::c_void,
    name: jni::objects::JString,
    down: jni::sys::jboolean,
    text: jni::sys::jint,
) {
    if name.is_null() {
        return;
    }
    let Ok(name) = env.get_string(&name) else {
        return;
    };
    let name = String::from(name);
    // An empty name is a character with no key of its own: what a launcher's keyboard types,
    // which the game takes through the same path GLFW's char callback feeds.
    if name.is_empty() {
        if let Some(text) = char::from_u32(text as u32) {
            horizon_client::android::push_char(text);
        }
        return;
    }
    // The queue keeps &'static str, and the set is fixed, so a known name is looked up once.
    let Some(name) = key_code_name(&name) else {
        return;
    };
    // 0 stands for "types nothing"; anything else is that code point.
    let text = char::from_u32(text as u32).filter(|_| text != 0);
    horizon_client::android::push_key(name, down != 0, text);
}

/// The `'static` name of a `KeyCode`, for the names a launcher's controls use. A launcher names
/// the key by what is printed on it, so this is the whole set it may ask for.
fn key_code_name(name: &str) -> Option<&'static str> {
    use winit::keyboard::KeyCode;
    const NAMES: &[(&str, KeyCode)] = &[
        ("KeyA", KeyCode::KeyA),
        ("KeyB", KeyCode::KeyB),
        ("KeyC", KeyCode::KeyC),
        ("KeyD", KeyCode::KeyD),
        ("KeyE", KeyCode::KeyE),
        ("KeyF", KeyCode::KeyF),
        ("KeyG", KeyCode::KeyG),
        ("KeyH", KeyCode::KeyH),
        ("KeyI", KeyCode::KeyI),
        ("KeyJ", KeyCode::KeyJ),
        ("KeyK", KeyCode::KeyK),
        ("KeyL", KeyCode::KeyL),
        ("KeyM", KeyCode::KeyM),
        ("KeyN", KeyCode::KeyN),
        ("KeyO", KeyCode::KeyO),
        ("KeyP", KeyCode::KeyP),
        ("KeyQ", KeyCode::KeyQ),
        ("KeyR", KeyCode::KeyR),
        ("KeyS", KeyCode::KeyS),
        ("KeyT", KeyCode::KeyT),
        ("KeyU", KeyCode::KeyU),
        ("KeyV", KeyCode::KeyV),
        ("KeyW", KeyCode::KeyW),
        ("KeyX", KeyCode::KeyX),
        ("KeyY", KeyCode::KeyY),
        ("KeyZ", KeyCode::KeyZ),
        ("Digit0", KeyCode::Digit0),
        ("Digit1", KeyCode::Digit1),
        ("Digit2", KeyCode::Digit2),
        ("Digit3", KeyCode::Digit3),
        ("Digit4", KeyCode::Digit4),
        ("Digit5", KeyCode::Digit5),
        ("Digit6", KeyCode::Digit6),
        ("Digit7", KeyCode::Digit7),
        ("Digit8", KeyCode::Digit8),
        ("Digit9", KeyCode::Digit9),
        ("Space", KeyCode::Space),
        ("ShiftLeft", KeyCode::ShiftLeft),
        ("ShiftRight", KeyCode::ShiftRight),
        ("ControlLeft", KeyCode::ControlLeft),
        ("ControlRight", KeyCode::ControlRight),
        ("AltLeft", KeyCode::AltLeft),
        ("AltRight", KeyCode::AltRight),
        ("Enter", KeyCode::Enter),
        ("Escape", KeyCode::Escape),
        ("Tab", KeyCode::Tab),
        ("Backspace", KeyCode::Backspace),
        ("F1", KeyCode::F1),
        ("F2", KeyCode::F2),
        ("F3", KeyCode::F3),
        ("F4", KeyCode::F4),
        ("F5", KeyCode::F5),
        ("ArrowUp", KeyCode::ArrowUp),
        ("ArrowDown", KeyCode::ArrowDown),
        ("ArrowLeft", KeyCode::ArrowLeft),
        ("ArrowRight", KeyCode::ArrowRight),
        ("PageUp", KeyCode::PageUp),
        ("PageDown", KeyCode::PageDown),
        ("Home", KeyCode::Home),
        ("End", KeyCode::End),
        ("Insert", KeyCode::Insert),
        ("Delete", KeyCode::Delete),
        ("CapsLock", KeyCode::CapsLock),
        ("F6", KeyCode::F6),
        ("F7", KeyCode::F7),
        ("F8", KeyCode::F8),
        ("F9", KeyCode::F9),
        ("F10", KeyCode::F10),
        ("F11", KeyCode::F11),
        ("F12", KeyCode::F12),
        ("Minus", KeyCode::Minus),
        ("Equal", KeyCode::Equal),
        ("BracketLeft", KeyCode::BracketLeft),
        ("BracketRight", KeyCode::BracketRight),
        ("Semicolon", KeyCode::Semicolon),
        ("Quote", KeyCode::Quote),
        ("Backquote", KeyCode::Backquote),
        ("Backslash", KeyCode::Backslash),
        ("Comma", KeyCode::Comma),
        ("Period", KeyCode::Period),
        ("Slash", KeyCode::Slash),
        ("NumLock", KeyCode::NumLock),
        ("ScrollLock", KeyCode::ScrollLock),
        ("NumpadEnter", KeyCode::NumpadEnter),
        ("NumpadDivide", KeyCode::NumpadDivide),
        ("Pause", KeyCode::Pause),
        ("Numpad0", KeyCode::Numpad0),
        ("Numpad1", KeyCode::Numpad1),
        ("Numpad2", KeyCode::Numpad2),
        ("Numpad3", KeyCode::Numpad3),
        ("Numpad4", KeyCode::Numpad4),
        ("Numpad5", KeyCode::Numpad5),
        ("Numpad6", KeyCode::Numpad6),
        ("Numpad7", KeyCode::Numpad7),
        ("Numpad8", KeyCode::Numpad8),
        ("Numpad9", KeyCode::Numpad9),
        ("NumpadAdd", KeyCode::NumpadAdd),
        ("NumpadSubtract", KeyCode::NumpadSubtract),
        ("NumpadDecimal", KeyCode::NumpadDecimal),
    ];
    NAMES.iter().find(|(n, _)| *n == name).map(|(n, _)| *n)
}

/// The system diagnostics window calls this on its worker, without starting the game.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_DiagnosticsActivity_makeReport(
    mut env: jni::JNIEnv,
    activity: jni::objects::JObject,
    private: jni::objects::JString,
    game: jni::objects::JString,
    export: jni::sys::jboolean,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<String, String> {
            let private = std::path::PathBuf::from(String::from(
                env.get_string(&private).map_err(|e| e.to_string())?,
            ));
            let game = std::path::PathBuf::from(String::from(
                env.get_string(&game).map_err(|e| e.to_string())?,
            ));
            horizon_client::android::recovery_report(
                &mut env,
                &activity,
                &private,
                &game,
                export != 0,
            )
        },
    ))
    .unwrap_or_else(|_| Err("Diagnostic worker failed".into()));
    match result {
        Ok(text) => env
            .new_string(text)
            .map_or(std::ptr::null_mut(), |text| text.into_raw()),
        Err(error) => {
            let _ = env.throw_new("java/io/IOException", error);
            std::ptr::null_mut()
        }
    }
}

/// `StarlingActivity.prepareResources`: FCL asks the engine to prepare the resources once its
/// download of the game's files is done, before the game is started. Runs on a thread of its
/// own; `preparer::stage` follows it, and `StarlingActivity.preparerStage` reads that.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_prepareResources(
    mut env: jni::JNIEnv,
    _class: *mut std::ffi::c_void,
    // output, minecraft_dir, libraries_dir, assets_dir, version_json, client_jar, server_jar,
    // java_home, kit, library_dir
    paths: jni::objects::JObjectArray,
) {
    if paths.is_null() {
        return;
    }
    let count = env.get_array_length(&paths).unwrap_or(0);
    let mut named: Vec<std::path::PathBuf> = Vec::with_capacity(count as usize);
    for i in 0..count {
        let Ok(value) = env.get_object_array_element(&paths, i) else {
            return;
        };
        let element = jni::objects::JString::from(value);
        let Ok(text) = env.get_string(&element) else {
            return;
        };
        named.push(std::path::PathBuf::from(String::from(text)));
    }
    if named.len() < 10 {
        horizon_client::android::trace(&format!(
            "prepare: {} paths given, 10 expected",
            named.len()
        ));
        return;
    }
    let paths = horizon_client::preparer::Paths {
        output: named[0].clone(),
        minecraft_dir: named[1].clone(),
        libraries_dir: named[2].clone(),
        assets_dir: named[3].clone(),
        version_json: named[4].clone(),
        client_jar: named[5].clone(),
        server_jar: named[6].clone(),
        java_home: named[7].clone(),
        kit: named[8].clone(),
        library_dir: named[9].clone(),
    };
    horizon_client::android::trace(&format!("prepare: asked for {:?}", paths.output));
    horizon_client::preparer::start_with(paths);
}

/// `StarlingActivity.preparerStage`: 0 idle, 1 fetching Java, 2 unpacking it, 3 preparing,
/// 4 done, 5 failed. The line or the reason goes to the log and the trace, which the launcher
/// already reads.
#[unsafe(no_mangle)]
extern "system" fn Java_io_github_juanluci_starling_StarlingActivity_preparerStage(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> jni::sys::jint {
    use horizon_client::preparer::Stage;
    let stage = horizon_client::preparer::stage();
    // Every change of stage is one line in the preparer's own log, which the launch screen and
    // the manage page both read: a String back through JNI is more machinery than this needs.
    let line = match &stage {
        Stage::Idle => return 0,
        Stage::Java(done, total) => format!(
            "Java 运行时 {} / {} MB",
            done / (1024 * 1024),
            total / (1024 * 1024)
        ),
        Stage::Unpacking => "解包 Java 运行时".to_owned(),
        Stage::Preparing(line) => line.clone(),
        Stage::Done => "完成".to_owned(),
        Stage::Failed(e) => format!("失败：{e}"),
    };
    horizon_client::android::preparer_line(&line);
    match stage {
        Stage::Idle => 0,
        Stage::Java(..) => 1,
        Stage::Unpacking => 2,
        Stage::Preparing(_) => 3,
        Stage::Done => 4,
        Stage::Failed(_) => 5,
    }
}
