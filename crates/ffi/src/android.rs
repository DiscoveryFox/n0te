//! Android glue outside UniFFI: hands the JVM and the application context to iroh, so it
//! reads the system's DNS servers (see `n0te_sync::install_android_context`).
//!
//! Kotlin: `NativeContext.install(context)` after `System.loadLibrary("n0te_ffi")`. JNA loads
//! the same library by name in the same linker namespace, so both share these statics; the
//! sync status reports `system_dns` so the app can show whether that worked.

use std::sync::atomic::{AtomicBool, Ordering};

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Whether the JNI context reached this copy of the library.
pub fn context_installed() -> bool {
    INSTALLED.load(Ordering::SeqCst)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_n0bridge_n0te_NativeContext_install(
    env: *mut jni_sys::JNIEnv,
    _class: jni_sys::jclass,
    context: jni_sys::jobject,
) -> jni_sys::jboolean {
    if env.is_null() || context.is_null() {
        return jni_sys::JNI_FALSE;
    }
    // ndk_context asserts it is set only once per process.
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return jni_sys::JNI_TRUE;
    }
    unsafe {
        let fns = &**env;
        let mut vm: *mut jni_sys::JavaVM = std::ptr::null_mut();
        let (Some(get_vm), Some(global_ref)) = (fns.GetJavaVM, fns.NewGlobalRef) else {
            INSTALLED.store(false, Ordering::SeqCst);
            return jni_sys::JNI_FALSE;
        };
        if get_vm(env, &mut vm) != jni_sys::JNI_OK || vm.is_null() {
            INSTALLED.store(false, Ordering::SeqCst);
            return jni_sys::JNI_FALSE;
        }
        // A global reference: the context must outlive this call (process lifetime).
        let global = global_ref(env, context);
        if global.is_null() {
            INSTALLED.store(false, Ordering::SeqCst);
            return jni_sys::JNI_FALSE;
        }
        n0te_sync::install_android_context(vm.cast(), global.cast());
    }
    jni_sys::JNI_TRUE
}
