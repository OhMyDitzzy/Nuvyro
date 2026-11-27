use jni::JNIEnv;
use jni::objects::{JByteArray, JClass};
use jni::sys::{jbyteArray, jint, jlong, jstring};

use crate::boundary;
use crate::command;
use crate::contract::{ABI_VERSION, MAX_COMMAND_BYTES, NativeStatus};
use crate::registry;

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativeAbiVersion(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jint {
    ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativeEngineVersion(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    boundary::throwing(&mut env, std::ptr::null_mut(), |env| {
        env.new_string(crate::ENGINE_VERSION)
            .map(|value| value.into_raw())
            .map_err(|_| NativeStatus::InternalFailure)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativeCreateEngine(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    abi: jint,
) -> jlong {
    boundary::throwing(&mut env, 0, |_| registry::create(abi))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativeDestroyEngine(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jint {
    boundary::status(&mut env, |_| registry::destroy(handle))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativeSubmit(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    command: JByteArray<'_>,
) -> jint {
    boundary::status(&mut env, |env| {
        let engine = registry::get(handle)?;
        let bytes = boundary::bytes(env, &command, MAX_COMMAND_BYTES)?;
        engine.submit(command::parse(&bytes)?)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativePoll(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    maximum: jint,
    timeout_ms: jint,
) -> jbyteArray {
    boundary::throwing(&mut env, std::ptr::null_mut(), |env| {
        let maximum = usize::try_from(maximum).map_err(|_| NativeStatus::InvalidArgument)?;
        registry::get(handle)?.poll_with(maximum, timeout_ms, |bytes| {
            let array = env
                .byte_array_from_slice(bytes)
                .map_err(|_| NativeStatus::InternalFailure)?;
            if env
                .exception_check()
                .map_err(|_| NativeStatus::InternalFailure)?
            {
                return Err(NativeStatus::InternalFailure);
            }
            Ok(array.into_raw())
        })
    })
}

fn reject_file_attachment(
    env: &JNIEnv<'_>,
    handle: jlong,
    generation: jlong,
    transfer: &JByteArray<'_>,
    file: &JByteArray<'_>,
    fd: jint,
) -> Result<(), NativeStatus> {
    let _engine = registry::get(handle)?;
    if generation <= 0 || fd < 0 {
        return Err(NativeStatus::InvalidArgument);
    }
    for array in [transfer, file] {
        let id = boundary::bytes(env, array, 16)?;
        if id.len() != 16 || id.iter().all(|byte| *byte == 0) {
            return Err(NativeStatus::InvalidArgument);
        }
    }
    // FD attachment requires an approved transfer/storage phase. Do not inspect, dup, or own a caller FD.
    Err(NativeStatus::InvalidState)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativeAttachSourceFd(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    generation: jlong,
    transfer: JByteArray<'_>,
    file: JByteArray<'_>,
    fd: jint,
) -> jint {
    boundary::status(&mut env, |env| {
        reject_file_attachment(env, handle, generation, &transfer, &file, fd)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ditzzy_nuvyro_core_transfer_NativeTransferBridge_nativeAttachDestinationFd(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    generation: jlong,
    transfer: JByteArray<'_>,
    file: JByteArray<'_>,
    fd: jint,
) -> jint {
    boundary::status(&mut env, |env| {
        reject_file_attachment(env, handle, generation, &transfer, &file, fd)
    })
}
