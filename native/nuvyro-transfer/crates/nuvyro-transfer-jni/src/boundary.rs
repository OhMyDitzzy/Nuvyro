use std::panic::{AssertUnwindSafe, catch_unwind};

use jni::JNIEnv;
use jni::objects::{JByteArray, JThrowable, JValue};

use crate::contract::NativeStatus;

pub(crate) fn catch<T>(
    operation: impl FnOnce() -> Result<T, NativeStatus>,
) -> Result<T, NativeStatus> {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or(Err(NativeStatus::InternalFailure))
}

pub(crate) fn throwing<'local, T>(
    env: &mut JNIEnv<'local>,
    fallback: T,
    operation: impl FnOnce(&mut JNIEnv<'local>) -> Result<T, NativeStatus>,
) -> T {
    match catch(|| operation(env)) {
        Ok(value) => value,
        Err(status) => {
            let _ = catch(|| {
                throw_status(env, status);
                Ok(())
            });
            fallback
        }
    }
}

pub(crate) fn status<'local>(
    env: &mut JNIEnv<'local>,
    operation: impl FnOnce(&mut JNIEnv<'local>) -> Result<(), NativeStatus>,
) -> i32 {
    match catch(|| operation(env)) {
        Ok(()) => NativeStatus::Accepted as i32,
        Err(status) => status as i32,
    }
}

fn throw_status(env: &mut JNIEnv<'_>, status: NativeStatus) {
    if env.exception_check().unwrap_or(true) {
        return;
    }
    match env.new_object(
        "com/ditzzy/nuvyro/core/transfer/NativeTransferException",
        "(I)V",
        &[JValue::Int(status as i32)],
    ) {
        Ok(value) => {
            let _ = env.throw(JThrowable::from(value));
        }
        Err(_) => {
            if !env.exception_check().unwrap_or(true) {
                let _ = env.throw_new(
                    "java/lang/IllegalStateException",
                    "Native transfer unavailable",
                );
            }
        }
    }
}

pub(crate) fn bytes(
    env: &JNIEnv<'_>,
    array: &JByteArray<'_>,
    maximum: usize,
) -> Result<Vec<u8>, NativeStatus> {
    if array.is_null() {
        return Err(NativeStatus::InvalidArgument);
    }
    let length = env
        .get_array_length(array)
        .map_err(|_| NativeStatus::InternalFailure)?;
    let length = usize::try_from(length).map_err(|_| NativeStatus::InvalidArgument)?;
    if length > maximum {
        return Err(NativeStatus::InvalidArgument);
    }
    env.convert_byte_array(array)
        .map_err(|_| NativeStatus::InternalFailure)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unwindable_panic_becomes_fixed_error() {
        assert_eq!(
            catch::<()>(|| panic!("test panic")),
            Err(NativeStatus::InternalFailure)
        );
    }
}
