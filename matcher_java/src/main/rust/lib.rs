use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jboolean, jint, jlong, jobjectArray, jsize, jstring};
use matcher_rs::{
    ProcessType, SimpleMatcher, SimpleTableSerde as SimpleTable,
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
};
use std::panic::{self, AssertUnwindSafe};

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_textProcess<'local>(
    env: JNIEnv<'local>,
    _class: JClass,
    process_type: jint,
    text_bytes: JByteArray,
) -> jstring {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let bytes = env.convert_byte_array(text_bytes).ok()?;
        let text_str = std::str::from_utf8(&bytes).ok()?;

        let p_type = ProcessType::from_bits_retain(process_type as u8);

        let res = text_process_rs(p_type, text_str);
        let j_string: JString = env.new_string(res.as_ref()).ok()?;
        Some(j_string.into_raw())
    }));

    result
        .unwrap_or_else(|_| {
            eprintln!("textProcess panicked");
            None
        })
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_reduceTextProcess<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    process_type: jint,
    text_bytes: JByteArray,
) -> jobjectArray {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let bytes = env.convert_byte_array(text_bytes).ok()?;
        let text_str = std::str::from_utf8(&bytes).ok()?;

        let p_type = ProcessType::from_bits_retain(process_type as u8);

        let variants = reduce_text_process_rs(p_type, text_str);

        let string_class = env.find_class("java/lang/String").ok()?;
        let initial_string = env.new_string("").ok()?;
        let obj_array = env
            .new_object_array(variants.len() as jsize, &string_class, initial_string)
            .ok()?;

        for (i, variant) in variants.iter().enumerate() {
            let j_str = env.new_string(variant.as_ref()).ok()?;
            env.set_object_array_element(&obj_array, i as jsize, j_str)
                .ok()?;
        }

        Some(obj_array.into_raw())
    }));

    result
        .unwrap_or_else(|_| {
            eprintln!("reduceTextProcess panicked");
            None
        })
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_initSimpleMatcher(
    env: JNIEnv,
    _class: JClass,
    simple_table_bytes: JByteArray,
) -> jlong {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let bytes = env.convert_byte_array(simple_table_bytes).ok()?;
        let simple_table: SimpleTable = sonic_rs::from_slice(&bytes).ok()?;
        let matcher = Box::new(SimpleMatcher::new(&simple_table));
        Some(Box::into_raw(matcher) as jlong)
    }));
    result
        .unwrap_or_else(|_| {
            eprintln!("initSimpleMatcher panicked");
            None
        })
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherIsMatch(
    env: JNIEnv,
    _class: JClass,
    matcher_ptr: jlong,
    text_bytes: JByteArray,
) -> jboolean {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if matcher_ptr == 0 {
            return Some(false);
        }
        let matcher = unsafe { &*(matcher_ptr as *const SimpleMatcher) };
        let bytes = env.convert_byte_array(text_bytes).ok()?;
        let text_str = std::str::from_utf8(&bytes).ok()?;
        Some(matcher.is_match(text_str))
    }));
    result
        .unwrap_or_else(|_| {
            eprintln!("simpleMatcherIsMatch panicked");
            None
        })
        .map_or(0, |b| b as jboolean)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherProcessAsString<'local>(
    env: JNIEnv<'local>,
    _class: JClass,
    matcher_ptr: jlong,
    text_bytes: JByteArray,
) -> jstring {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if matcher_ptr == 0 {
            return None;
        }
        let matcher = unsafe { &*(matcher_ptr as *const SimpleMatcher) };
        let bytes = env.convert_byte_array(text_bytes).ok()?;
        let text_str = std::str::from_utf8(&bytes).ok()?;
        let res = matcher.process(text_str);
        let res_json = sonic_rs::to_string(&res).ok()?;
        let j_string: JString = env.new_string(res_json).ok()?;
        Some(j_string.into_raw())
    }));
    result
        .unwrap_or_else(|_| {
            eprintln!("simpleMatcherProcessAsString panicked");
            None
        })
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_dropSimpleMatcher(
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
