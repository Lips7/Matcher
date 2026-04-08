use std::ptr;

use jni::{
    Env, EnvUnowned,
    errors::{Error as JniError, Result as JniResult, ThrowRuntimeExAndDefault},
    objects::{JByteArray, JClass, JMethodID, JObject, JObjectArray},
    sys::{jboolean, jbooleanArray, jint, jlong, jobject, jobjectArray, jstring, jvalue},
};
use matcher_rs::{
    ProcessType, SimpleMatcher, SimpleTableSerde as SimpleTable,
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
};

macro_rules! simple_result_class {
    () => {
        jni::jni_str!("com/matcherjava/extensiontypes/SimpleResult")
    };
}
macro_rules! simple_result_init_sig {
    () => {
        jni::jni_sig!("(ILjava/lang/String;)V")
    };
}
macro_rules! simple_result_array_class {
    () => {
        jni::jni_str!("[Lcom/matcherjava/extensiontypes/SimpleResult;")
    };
}

fn decode_text(env: &Env<'_>, text_bytes: JByteArray<'_>) -> JniResult<String> {
    String::from_utf8(env.convert_byte_array(text_bytes)?)
        .map_err(|error| JniError::ParseFailed(error.to_string()))
}

fn parse_simple_table(simple_table_bytes: &[u8]) -> JniResult<SimpleTable<'_>> {
    sonic_rs::from_slice(simple_table_bytes)
        .map_err(|error| JniError::ParseFailed(error.to_string()))
}

fn process_type_from_jint(process_type: jint) -> ProcessType {
    ProcessType::from_bits_retain(process_type as u8)
}

fn decode_texts(env: &mut Env<'_>, texts_array: &JObjectArray<'_>) -> JniResult<Vec<String>> {
    let len = texts_array.len(env)?;
    let mut texts = Vec::with_capacity(len);
    for i in 0..len {
        let element = texts_array.get_element(env, i)?;
        // SAFETY: the Java caller passes byte[][] — each element is a byte[].
        let byte_array = unsafe { JByteArray::from_raw(env, element.into_raw()) };
        texts.push(decode_text(env, byte_array)?);
    }
    Ok(texts)
}

fn matcher_from_ptr(matcher_ptr: jlong) -> Option<&'static SimpleMatcher> {
    if matcher_ptr == 0 {
        return None;
    }

    Some(unsafe { &*(matcher_ptr as *const SimpleMatcher) })
}

fn build_result_object<'a>(
    env: &mut Env<'a>,
    class: &JClass<'a>,
    init: JMethodID,
    result: &matcher_rs::SimpleResult<'_>,
) -> JniResult<JObject<'a>> {
    let word = env.new_string(result.word.as_ref())?;
    // SAFETY: `init` is the (int, String) constructor resolved from `class`.
    unsafe {
        env.new_object_unchecked(
            class,
            init,
            &[
                jvalue {
                    i: result.word_id as jint,
                },
                jvalue { l: word.into_raw() },
            ],
        )
    }
}

fn build_result_array<'a>(
    env: &mut Env<'a>,
    class: &JClass<'a>,
    init: JMethodID,
    results: &[matcher_rs::SimpleResult<'_>],
) -> JniResult<JObjectArray<'a>> {
    let array = env.new_object_array(results.len() as i32, class, JObject::null())?;
    for (i, r) in results.iter().enumerate() {
        let obj = build_result_object(env, class, init, r)?;
        array.set_element(env, i, &obj)?;
    }
    Ok(array)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_textProcess<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    process_type: jint,
    text_bytes: JByteArray<'local>,
) -> jstring {
    env.with_env(|env| -> JniResult<_> {
        let text = decode_text(env, text_bytes)?;
        let processed = text_process_rs(process_type_from_jint(process_type), &text);

        Ok(env.new_string(processed.as_ref())?.into_raw())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_reduceTextProcess<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    process_type: jint,
    text_bytes: JByteArray<'local>,
) -> jobjectArray {
    env.with_env(|env| -> JniResult<_> {
        let text = decode_text(env, text_bytes)?;
        let variants = reduce_text_process_rs(process_type_from_jint(process_type), &text);
        let array = env.new_object_array(
            variants.len() as i32,
            jni::jni_str!("java/lang/String"),
            JObject::null(),
        )?;

        for (index, variant) in variants.iter().enumerate() {
            let value = env.new_string(variant.as_ref())?;
            array.set_element(env, index, &value)?;
        }

        Ok(array.into_raw())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_initSimpleMatcher<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    simple_table_bytes: JByteArray<'local>,
) -> jlong {
    env.with_env(|env| -> JniResult<_> {
        let bytes = env.convert_byte_array(simple_table_bytes)?;
        let simple_table = parse_simple_table(&bytes)?;
        let matcher = Box::new(
            SimpleMatcher::new(&simple_table).map_err(|e| JniError::ParseFailed(e.to_string()))?,
        );

        Ok(Box::into_raw(matcher) as jlong)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherIsMatch<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    text_bytes: JByteArray<'local>,
) -> jboolean {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(false);
        };

        let text = decode_text(env, text_bytes)?;
        Ok(matcher.is_match(&text))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherProcess<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    text_bytes: JByteArray<'local>,
) -> jobjectArray {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(ptr::null_mut());
        };

        let text = decode_text(env, text_bytes)?;
        let results = matcher.process(&text);

        let class = env.find_class(simple_result_class!())?;
        let init = env.get_method_id(&class, jni::jni_str!("<init>"), simple_result_init_sig!())?;
        Ok(build_result_array(env, &class, init, &results)?.into_raw())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherFindMatch<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    text_bytes: JByteArray<'local>,
) -> jobject {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(ptr::null_mut());
        };

        let text = decode_text(env, text_bytes)?;
        match matcher.find_match(&text) {
            Some(result) => {
                let class = env.find_class(simple_result_class!())?;
                let init =
                    env.get_method_id(&class, jni::jni_str!("<init>"), simple_result_init_sig!())?;
                Ok(build_result_object(env, &class, init, &result)?.into_raw())
            }
            None => Ok(ptr::null_mut()),
        }
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherBatchIsMatch<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    texts_bytes: JObjectArray<'local>,
) -> jbooleanArray {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(ptr::null_mut());
        };

        let texts = decode_texts(env, &texts_bytes)?;
        let results: Vec<bool> = texts.iter().map(|t| matcher.is_match(t)).collect();
        let array = env.new_boolean_array(results.len())?;
        array.set_region(env, 0, &results)?;

        Ok(array.into_raw())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherBatchProcess<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    texts_bytes: JObjectArray<'local>,
) -> jobjectArray {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(ptr::null_mut());
        };

        let texts = decode_texts(env, &texts_bytes)?;
        let class = env.find_class(simple_result_class!())?;
        let init = env.get_method_id(&class, jni::jni_str!("<init>"), simple_result_init_sig!())?;
        let array_class = env.find_class(simple_result_array_class!())?;

        let outer = env.new_object_array(texts.len() as i32, &array_class, JObject::null())?;
        for (i, text) in texts.iter().enumerate() {
            let results = matcher.process(text);
            let inner = build_result_array(env, &class, init, &results)?;
            outer.set_element(env, i, &inner)?;
        }

        Ok(outer.into_raw())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherBatchFindMatch<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    texts_bytes: JObjectArray<'local>,
) -> jobjectArray {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(ptr::null_mut());
        };

        let texts = decode_texts(env, &texts_bytes)?;
        let class = env.find_class(simple_result_class!())?;
        let init = env.get_method_id(&class, jni::jni_str!("<init>"), simple_result_init_sig!())?;

        let array = env.new_object_array(texts.len() as i32, &class, JObject::null())?;
        for (i, text) in texts.iter().enumerate() {
            if let Some(result) = matcher.find_match(text) {
                let obj = build_result_object(env, &class, init, &result)?;
                array.set_element(env, i, &obj)?;
            }
        }

        Ok(array.into_raw())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_matcherjava_MatcherJava_dropSimpleMatcher<'local>(
    _env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
) {
    if matcher_ptr != 0 {
        unsafe { drop(Box::from_raw(matcher_ptr as *mut SimpleMatcher)) };
    }
}
