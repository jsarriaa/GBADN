//! The console's entry point on Android.

use eframe::egui_winit::winit::platform::android::activity::AndroidApp;
use jni::JavaVM;
use jni::objects::JObject;

use crate::app::Console;
use crate::audio::Speaker;
use crate::controls::Gamepads;

/// Android starts here, loading this library through `android.app.lib_name`.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("gbadn"),
    );
    std::panic::set_hook(Box::new(|panic| log::error!("{panic}")));

    let storage = app
        .internal_data_path()
        .expect("Android gives apps storage");
    let picker_app = app.clone();
    let options = eframe::NativeOptions {
        android_app: Some(app),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let result = eframe::run_native(
        "GBADN Console",
        options,
        Box::new(move |_| {
            Ok(Box::new(
                Console::new(Speaker::open())
                    .with_gamepads(Gamepads::new())
                    .with_touch_controls()
                    .with_file_picker(
                        Box::new(move || ask_for_cartridge(&picker_app)),
                        storage.join("pending_cartridge"),
                    ),
            ))
        }),
    );
    if let Err(error) = result {
        log::error!("{error}");
    }
}

/// Asks `GbadnActivity` to show the system's file picker.
fn ask_for_cartridge(app: &AndroidApp) {
    // SAFETY: Android keeps the VM and activity alive for as long as this app runs.
    let result = unsafe {
        JavaVM::from_raw(app.vm_as_ptr().cast()).and_then(|vm| {
            let mut env = vm.attach_current_thread_permanently()?;
            let activity = JObject::from_raw(app.activity_as_ptr().cast());
            env.call_method(&activity, "pickCartridge", "()V", &[])?;
            Ok(())
        })
    };
    if let Err(error) = result {
        log::error!("could not open the file picker: {error}");
    }
}
