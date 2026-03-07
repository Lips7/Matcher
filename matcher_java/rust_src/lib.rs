use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jboolean, jint, jlong, jstring};
use matcher_rs::{ProcessType, SimpleMatcher, SimpleTableSerde as SimpleTable, reduce_text_process as reduce_text_process_rs, text_process as text_process_rs};
use std::panic::{self, AssertUnwindSafe};

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcher_1java_MatcherJava_textProcess<'local>(
    env: JNIEnv<'local>,
    _class: JClass,
    process_type: jint,
    text_bytes: JByteArray,
) -> jstring {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let bytes = env.convert_byte_array(text_bytes).unwrap();
        let text_str = std::str::from_utf8(&bytes).unwrap();

        let p_type = ProcessType::from_bits(process_type as u8).unwrap_or(ProcessType::None);

        match text_process_rs(p_type, text_str) {
            Ok(res) => {
                let j_string: JString = env.new_string(res.as_ref()).unwrap();
                j_string.into_raw()
            }
            Err(_) => std::ptr::null_mut(),
        }
    }));

    result.unwrap_or_else(|_| {
        eprintln!("textProcess failed");
        std::ptr::null_mut()
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcher_1java_MatcherJava_reduceTextProcess<'local>(
    env: JNIEnv<'local>,
    _class: JClass,
    process_type: jint,
    text_bytes: JByteArray,
) -> jstring {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let bytes = env.convert_byte_array(text_bytes).unwrap();
        let text_str = std::str::from_utf8(&bytes).unwrap();

        let p_type = ProcessType::from_bits(process_type as u8).unwrap_or(ProcessType::None);

        let variants = reduce_text_process_rs(p_type, text_str);

        let res_json = sonic_rs::to_string(&variants).unwrap();
        let j_string: JString = env.new_string(res_json).unwrap();
        j_string.into_raw()
    }));

    result.unwrap_or_else(|_| {
        eprintln!("reduceTextProcess failed");
        std::ptr::null_mut()
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcher_1java_MatcherJava_initSimpleMatcher(
    env: JNIEnv,
    _class: JClass,
    simple_table_bytes: JByteArray,
) -> jlong {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let bytes = env.convert_byte_array(simple_table_bytes).unwrap();
        let simple_table: SimpleTable = sonic_rs::from_slice(&bytes).unwrap();
        let matcher = Box::new(SimpleMatcher::new(&simple_table));
        Box::into_raw(matcher) as jlong
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            eprintln!("initSimpleMatcher failed");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcher_1java_MatcherJava_simpleMatcherIsMatch(
    env: JNIEnv,
    _class: JClass,
    matcher_ptr: jlong,
    text_bytes: JByteArray,
) -> jboolean {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if matcher_ptr == 0 {
            return false;
        }
        let matcher = unsafe { &*(matcher_ptr as *mut SimpleMatcher) };
        let bytes = env.convert_byte_array(text_bytes).unwrap();
        let text_str = std::str::from_utf8(&bytes).unwrap();
        matcher.is_match(text_str)
    }));
    match result {
        Ok(res) => if res { 1 } else { 0 },
        Err(_) => {
            eprintln!("simpleMatcherIsMatch failed");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcher_1java_MatcherJava_simpleMatcherProcessAsString<'local>(
    env: JNIEnv<'local>,
    _class: JClass,
    matcher_ptr: jlong,
    text_bytes: JByteArray,
) -> jstring {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if matcher_ptr == 0 {
            return std::ptr::null_mut();
        }
        let matcher = unsafe { &*(matcher_ptr as *mut SimpleMatcher) };
        let bytes = env.convert_byte_array(text_bytes).unwrap();
        let text_str = std::str::from_utf8(&bytes).unwrap();
        let res = matcher.process(text_str);
        let res_json = sonic_rs::to_string(&res).unwrap();
        let j_string: JString = env.new_string(res_json).unwrap();
        j_string.into_raw()
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            eprintln!("simpleMatcherProcessAsString failed");
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcher_1java_MatcherJava_dropSimpleMatcher(
    _env: JNIEnv,
    _class: JClass,
    matcher_ptr: jlong,
) {
    if matcher_ptr != 0 {
        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            unsafe { drop(Box::from_raw(matcher_ptr as *mut SimpleMatcher)) };
        }));
    }
}
