//! The game files prepared on the device, where no computer does it (Android): once the sign-in
//! has fetched the official files (accounts.rs), the Java runtime (starling_account::java) and
//! the preparer's kit the app carries (`assets/prepare-kit.zip`, build_apk.py), then
//! horizon-prepare over those files with the app's Java launchers. What it makes becomes the
//! game's resources folder; the runtime goes once it is done.
#[path = "preparer_install.rs"]
mod install;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

/// Every path the preparation reads and writes, named by the launcher that asked for it: the
/// launcher knows where its own install, its Java and its jars are, and nothing here has to
/// guess at a platform hook or an environment variable that is not set before a game starts.
#[derive(Clone, Debug)]
pub struct Paths {
    /// Where the engine reads the resources (`. . ./versions/<id>/resources`).
    pub output: PathBuf,
    /// The game's install: `versions/`, `libraries/`, `assets/`.
    pub minecraft_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub version_json: PathBuf,
    pub client_jar: PathBuf,
    /// The pinned server jar the preparation exports from.
    pub server_jar: PathBuf,
    /// The Java runtime the exporters run on (its `bin/java`, `bin/javac`).
    pub java_home: PathBuf,
    /// The unpacked preparation kit: `research-lock.json`, the exporters' sources and classes.
    pub kit: PathBuf,
    /// Where the APK's native libraries are (`libstarling_java.so`, `libstarling_javac.so`).
    pub library_dir: PathBuf,
}

/// Where the preparing is.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Stage {
    #[default]
    Idle,
    /// Fetching the Java runtime: bytes so far, and in all.
    Java(u64, u64),
    /// Unpacking it.
    Unpacking,
    /// horizon-prepare at work: its last line.
    Preparing(String),
    /// The resources are in place.
    Done,
    Failed(String),
}

fn shared() -> MutexGuard<'static, Stage> {
    static STAGE: OnceLock<Mutex<Stage>> = OnceLock::new();
    STAGE
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Where the preparing is now.
pub fn stage() -> Stage {
    shared().clone()
}

/// Back to idle once what it made was taken (the game loads it).
pub fn clear() {
    let mut stage = shared();
    if *stage == Stage::Done {
        *stage = Stage::Idle;
    }
}

/// Whether it is under way.
pub fn running() -> bool {
    matches!(
        stage(),
        Stage::Java(..) | Stage::Unpacking | Stage::Preparing(_)
    )
}

/// The runtime, the kit and horizon-prepare's work files: the app's own folder.
fn home() -> PathBuf {
    crate::logging::private_home().join("preparer")
}

/// Prepares into `paths.output` with every path named by the launcher: the same work as
/// [`start`], without asking a platform hook or an account module for any of it, so it runs
/// before the engine has ever started.
pub fn start_with(paths: Paths) {
    {
        let mut stage = shared();
        if matches!(
            *stage,
            Stage::Java(..) | Stage::Unpacking | Stage::Preparing(_)
        ) {
            return;
        }
        *stage = Stage::Unpacking;
    }
    let spawned = std::thread::Builder::new()
        .name("preparer".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let result = run_with(&paths);
            let detail = match &result {
                Ok(()) => "the resources were made".to_owned(),
                Err(e) => format!("FAILED: {e}"),
            };
            write_stage_line(&detail);
            *shared() = match result {
                Ok(()) => {
                    log::info!(
                        "Horizon: the game files were prepared in {} s",
                        started.elapsed().as_secs()
                    );
                    Stage::Done
                }
                Err(e) => {
                    log::warn!("Horizon: the game files were not prepared: {e}");
                    Stage::Failed(e)
                }
            };
        });
    if let Err(e) = spawned {
        *shared() = Stage::Failed(e.to_string());
    }
}

/// [`start_with`]'s work, over the paths the launcher named.
fn run_with(paths: &Paths) -> Result<(), String> {
    let temp = paths.kit.join("tmp");
    std::fs::create_dir_all(&temp).map_err(|e| format!("{}: {e}", temp.display()))?;
    let env = |path: &Path| path.display().to_string();
    horizon_prepare::java::supply(horizon_prepare::java::Supplied {
        java: paths.library_dir.join("libstarling_java.so"),
        javac: paths.library_dir.join("libstarling_javac.so"),
        env: vec![
            ("STARLING_JAVA_HOME".into(), env(&paths.java_home)),
            (
                "LD_LIBRARY_PATH".into(),
                format!(
                    "{}:{}",
                    env(&paths.java_home.join("lib")),
                    env(&paths.library_dir)
                ),
            ),
            ("JAVA_HOME".into(), env(&paths.java_home)),
            ("TMPDIR".into(), env(&temp)),
            (
                "JAVA_TOOL_OPTIONS".into(),
                format!("-Djava.io.tmpdir={}", env(&temp)),
            ),
        ],
    });
    if let Some(limit) = horizon_prepare::java::max_heap_mib() {
        log::info!("Horizon: 32-bit preparation Java heap limit: {limit} MiB");
    }
    *shared() = Stage::Preparing("正在导入游戏文件".into());
    let made = horizon_prepare::prepare(
        &horizon_prepare::Options {
            root: paths.kit.clone(),
            import: true,
            minecraft_dir: Some(paths.minecraft_dir.clone()),
            server_jar: Some(paths.server_jar.clone()),
            java_home: Some(paths.java_home.clone()),
            inputs: horizon_prepare::artifacts::Inputs {
                version_json: Some(paths.version_json.clone()),
                client_jar: Some(paths.client_jar.clone()),
                libraries_dir: Some(paths.libraries_dir.clone()),
                assets_dir: Some(paths.assets_dir.clone()),
            },
            output: Some(paths.output.clone()),
        },
        &|line| {
            *shared() = Stage::Preparing(line.to_owned());
        },
    )?;
    if !made.is_dir() {
        return Err(format!(
            "the preparation wrote nothing to {}",
            made.display()
        ));
    }
    // The sounds come from the game's own assets, and the preparation does not carry them: the
    // game reads them from the prepared folder, so they are added here, from the same install.
    *shared() = Stage::Preparing("复制音效".into());
    horizon_prepare::sounds(&paths.kit, Some(&paths.minecraft_dir), &|line| {
        *shared() = Stage::Preparing(line.to_owned());
    })
    .map_err(|e| format!("the sounds were not added: {e}"))?;
    Ok(())
}

/// Prepare the fetched files into `resources` (the folder the game reads), on a thread of its
/// own; `stage` follows it.
pub fn start(resources: PathBuf) {
    let Some(runtime) = starling_account::java::android_runtime(std::env::consts::ARCH) else {
        *shared() = Stage::Failed("On 32-bit x86, import prepared resources as a .starling file. An Android x86 Java 25 runtime is not available.".into());
        return;
    };
    {
        let mut stage = shared();
        if matches!(
            *stage,
            Stage::Java(..) | Stage::Unpacking | Stage::Preparing(_)
        ) {
            return;
        }
        *stage = Stage::Java(0, runtime.size);
    }
    let spawned = std::thread::Builder::new()
        .name("preparer".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let result = run(&resources);
            let detail = match &result {
                Ok(()) => "the resources were made".to_owned(),
                Err(e) => format!("FAILED: {e}"),
            };
            write_stage_line(&detail);
            *shared() = match result {
                Ok(()) => {
                    log::info!(
                        "Horizon: the game files were prepared in {} s",
                        started.elapsed().as_secs()
                    );
                    Stage::Done
                }
                Err(e) => {
                    log::warn!("Horizon: the game files were not prepared: {e}");
                    Stage::Failed(e)
                }
            };
        });
    if let Err(e) = spawned {
        *shared() = Stage::Failed(e.to_string());
    }
}

/// The launcher's own Java 25 (`app_runtime/java/jre25` in the app's files), the one FCL unpacks
/// for the games it runs: the preparer needs a Java and this is the one already on the device.
fn launcher_java() -> Option<PathBuf> {
    // FCL unpacks its runtimes beside `files/`, not inside it: the app's own data folder, one
    // level up from HORIZON_PRIVATE.
    let files = crate::logging::private_home();
    let base = files.parent().map(Path::to_path_buf).unwrap_or(files);
    let runtime = base.join("app_runtime").join("java");
    let direct = runtime.join("jre25");
    if direct.join("bin").join("java").is_file() {
        return Some(direct);
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(&runtime)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.join("bin").join("java").is_file())
        .collect();
    found.sort();
    found.pop()
}

/// The preparer's outcome, where the launcher reads it: its own files, and the folder a phone's
/// file manager reaches. A plain file write, so this crate and the lib it is shared with need
/// no Android module between them.
fn write_stage_line(line: &str) {
    let files = crate::logging::private_home();
    let _ = std::fs::write(files.join("preparer-stage.txt"), line);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/storage/emulated/0/Starling/.starling-resources.log")
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

fn run(resources: &Path) -> Result<(), String> {
    let runtime = starling_account::java::android_runtime(std::env::consts::ARCH)
        .ok_or("No Java runtime is available for this architecture.")?;
    let server_jar = crate::accounts::server_jar()
        .ok_or("The game files are not all there: download them again.")?;
    let library = crate::platform::library_dir().ok_or("Starling cannot find its own files.")?;
    let home = home();
    let kit = home.join("kit");
    unpack_kit(&kit)?;
    if !horizon_prepare::exporters::available(&kit) {
        return Err("The app's exporter bundle is missing, outdated or damaged. Install the current APK again.".into());
    }
    let jre = home.join("jre25");
    // The launcher's own runtime, when it has one: FCL unpacks a Java 25 into the app's files
    // for the game it hosts, and making the resources needs the same one. Only when it is not
    // there does the pinned runtime get fetched (38 MB, for a phone with no launcher of its own).
    let found_java = launcher_java();
    if found_java.is_none() {
        write_stage_line(&format!(
            "no launcher Java under {:?}",
            crate::logging::private_home().parent()
        ));
    }
    let jre_dir = match found_java {
        Some(own) => {
            *shared() = Stage::Unpacking;
            log::info!("Horizon: the launcher's Java {} is used", own.display());
            own
        }
        None => {
            starling_account::java::install(
                runtime,
                &jre,
                &|done, total| {
                    *shared() = if done >= total {
                        Stage::Unpacking
                    } else {
                        Stage::Java(done, total)
                    };
                },
                &|| false,
            )?;
            jre
        }
    };
    let temp = home.join("tmp");
    std::fs::create_dir_all(&temp).map_err(|e| format!("{}: {e}", temp.display()))?;
    let env = |path: &Path| path.display().to_string();
    horizon_prepare::java::supply(horizon_prepare::java::Supplied {
        java: library.join("libstarling_java.so"),
        javac: library.join("libstarling_javac.so"),
        env: vec![
            ("STARLING_JAVA_HOME".into(), env(&jre_dir)),
            // The runtime's libraries need one another by name (libnio.so libnet.so): Android's
            // linker finds them on this path only. Not lib/server, libjvm.so's (loaded by its
            // path): with that on it the launcher runs the runtime's bin/java again, which an
            // app may not.
            (
                "LD_LIBRARY_PATH".into(),
                format!("{}:{}", env(&jre_dir.join("lib")), env(&library)),
            ),
            ("JAVA_HOME".into(), env(&jre_dir)),
            ("TMPDIR".into(), env(&temp)),
            (
                "JAVA_TOOL_OPTIONS".into(),
                format!("-Djava.io.tmpdir={}", env(&temp)),
            ),
        ],
    });
    if let Some(limit) = horizon_prepare::java::max_heap_mib() {
        log::info!("Horizon: 32-bit preparation Java heap limit: {limit} MiB");
    }
    *shared() = Stage::Preparing("Starting Java".into());
    // A launcher's install names its files one by one: the version JSON there is the launcher's
    // own (FCL rewrites it, so its hash is not the one the pinned manifest has), while the jar,
    // the libraries and the assets are Mojang's. Naming them skips the folder scan that would
    // reject the rewritten JSON.
    let root = crate::accounts::files_root();
    let inputs = launcher_inputs(&root);
    let made = horizon_prepare::prepare(
        &horizon_prepare::Options {
            root: kit.clone(),
            import: true,
            minecraft_dir: Some(root),
            server_jar: Some(server_jar),
            java_home: None,
            inputs,
            output: None,
        },
        &|line| {
            log::info!("Horizon: preparing: {line}");
            *shared() = Stage::Preparing(line.to_owned());
        },
    )?;
    // The sounds: `prepare` leaves them out (the desktop launcher copies them after it,
    // backend.rs). Without them the game plays, silent.
    copy_sounds(&kit)?;
    let missing = horizon_prepare::check(&kit, &made)?;
    if !missing.is_empty() {
        return Err(format!(
            "Prepared resources are incomplete: {}",
            missing.join("; ")
        ));
    }
    install::activate(&made, resources)?;
    // Bedrock's touch buttons, from Mojang too, so the first game already has them.
    crate::bedrock_touch::fetch_logged(resources);
    // Preparation replaces the resource directory. Fetch the original control
    // overlay afterwards so the first game, not just a restart, receives it.
    crate::android_control_assets::fetch(resources, |done, total| {
        *shared() = Stage::Preparing(format!("Control textures: {done}/{total}"));
    })
    .map_err(|e| format!("Control textures: {e}"))?;
    // Everything the game reads is in the resources now: the runtime and the work files go.
    for done in [&jre_dir, &kit.join(".local"), &temp] {
        let _ = std::fs::remove_dir_all(done);
    }
    Ok(())
}

/// The files of a launcher's own install under `root` (`versions/<v>/`, `libraries/`,
/// `assets/`), named for horizon-prepare: the ones it would otherwise search for and check by
/// hash, which a launcher's rewritten version JSON fails.
fn launcher_inputs(root: &std::path::Path) -> horizon_prepare::artifacts::Inputs {
    use horizon_prepare::artifacts::Inputs;
    let version = starling_account::files::Pins::starling().version;
    let folder = root.join("versions").join(&version);
    if !folder.is_dir() {
        return Inputs::default();
    }
    let mut inputs = Inputs {
        version_json: None,
        client_jar: None,
        libraries_dir: None,
        assets_dir: None,
    };
    // Mojang's own JSON, when the app kept one: the launcher's version JSON is rewritten (FCL's
    // hash is not the pinned one), so the original is read where the app saved it instead.
    for pinned in [
        crate::logging::private_home()
            .join("minecraft")
            .join(&version)
            .join("manifest.json"),
        root.join("manifest.json"),
    ] {
        if pinned.is_file() {
            inputs.version_json = Some(pinned);
            break;
        }
    }
    let jar = folder.join(format!("{version}.jar"));
    if jar.is_file() {
        inputs.client_jar = Some(jar);
    }
    let libraries = root.join("libraries");
    if libraries.is_dir() {
        inputs.libraries_dir = Some(libraries);
    }
    let assets = root.join("assets");
    if assets.is_dir() {
        inputs.assets_dir = Some(assets);
    }
    inputs
}

/// The fetched files' sounds into `kit`'s prepared folder (horizon-prepare's `sounds`).
fn copy_sounds(kit: &Path) -> Result<(), String> {
    horizon_prepare::sounds(kit, Some(&crate::accounts::files_root()), &|line| {
        log::info!("Horizon: preparing: {line}");
        *shared() = Stage::Preparing(line.to_owned());
    })
    .map(drop)
}

/// Starling before 0.1.6 prepared the game without its sounds: once the fetched files are
/// there, they are added to the prepared `resources`, before the game reads its sounds.
pub fn complete_sounds(resources: &Path) {
    if !crate::platform::registered()
        || !resources.is_dir()
        || resources.join("assets/minecraft/sounds.json").is_file()
        || crate::accounts::files() != crate::accounts::Files::Ready
    {
        return;
    }
    match add_sounds(resources) {
        Ok(()) => log::info!(
            "Horizon: the game's sounds were added to {}",
            resources.display()
        ),
        Err(e) => log::warn!("Horizon: the game's sounds were not added: {e}"),
    }
}

/// horizon-prepare copies into the kit's prepared folder: a link to `resources` stands in for
/// it while it does.
#[cfg(unix)]
fn add_sounds(resources: &Path) -> Result<(), String> {
    let kit = home().join("kit");
    unpack_kit(&kit)?;
    let version = horizon_prepare::artifacts::Lock::read(&kit)?.version;
    let target = kit.join(".local/horizon-client").join(&version);
    // A link left by a stop midway, or the work of a preparation cut short.
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_dir_all(&target);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::os::unix::fs::symlink(resources, &target)
        .map_err(|e| format!("{}: {e}", target.display()))?;
    let copied = horizon_prepare::sounds(&kit, Some(&crate::accounts::files_root()), &|line| {
        log::info!("Horizon: adding the sounds: {line}")
    });
    let _ = std::fs::remove_file(&target);
    copied.map(drop)
}

#[cfg(not(unix))]
fn add_sounds(_: &Path) -> Result<(), String> {
    Err("only a phone's setup prepares on the device".into())
}

/// The low-budget skin the setup draws the game's menus with while their own resources are not
/// there (packaging/lowbudget, the APK's `assets/lowbudget.zip`): a resource folder, unpacked
/// into the app's own folder once for each skin the app carries.
pub fn skin() -> Option<PathBuf> {
    let bytes = crate::platform::asset("lowbudget.zip")?;
    let stamp = format!("{}-{:08x}", bytes.len(), crc32(&bytes));
    let dir = home().join("lowbudget");
    let marker = dir.join(".starling-skin");
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(&stamp) {
        return Some(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    match unpack(&bytes, &dir)
        .and_then(|()| std::fs::write(&marker, &stamp).map_err(|e| e.to_string()))
    {
        Ok(()) => Some(dir),
        Err(e) => {
            log::warn!("Horizon: the setup's skin did not unpack: {e}");
            None
        }
    }
}

/// Starling Visuals, the shader pack the app carries (packaging/shaderpacks/starling-visuals, the
/// APK's `assets/StarlingVisuals.zip`), put in `shaderpacks/` of the game folder: again only when
/// the app carries another one than it put there last, so a pack the player deleted stays deleted
/// until an update brings a new one.
pub fn put_shader_pack(game_folder: &Path) {
    let Some(bytes) = crate::platform::asset("StarlingVisuals.zip") else {
        return;
    };
    let stamp = format!("{}-{:08x}", bytes.len(), crc32(&bytes));
    let marker = crate::logging::private_home().join(".starling-visuals");
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(&stamp) {
        return;
    }
    let dir = game_folder.join("shaderpacks");
    match std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join("StarlingVisuals.zip"), &bytes))
        .and_then(|()| std::fs::write(&marker, &stamp))
    {
        Ok(()) => log::info!("Horizon: Starling Visuals put in {}", dir.display()),
        Err(e) => log::warn!(
            "Horizon: Starling Visuals could not be put in {}: {e}",
            dir.display()
        ),
    }
}

/// CRC-32 (IEEE), to tell one skin from the next.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The kit the app carries, over `dir` (its sources replaced, horizon-prepare's work kept).
fn unpack_kit(dir: &Path) -> Result<(), String> {
    let bytes = crate::platform::asset("prepare-kit.zip")
        .ok_or("Starling's preparer is missing from the app: install it again.")?;
    install::reset_kit_sources(dir)?;
    unpack(&bytes, dir)
}

/// A zip the app carries over `dir`, its entries kept inside it.
fn unpack(bytes: &[u8], dir: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            return Err(format!(
                "The app's archive names {}",
                entry
                    .name()
                    .map_or_else(|e| e.to_string(), |n| n.into_owned())
            ));
        };
        let path = dir.join(name);
        if entry.is_dir() {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mut file =
            std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        std::io::copy(&mut entry, &mut file).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}
