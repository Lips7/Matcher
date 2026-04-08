use std::ptr;

use jni::{
    Env, EnvUnowned,
    errors::{Error as JniError, Result as JniResult, ThrowRuntimeExAndDefault},
    objects::{JByteArray, JClass, JObject, JObjectArray},
    sys::{jboolean, jbooleanArray, jint, jlong, jobjectArray, jstring},
};
use matcher_rs::{
    ProcessType, SimpleMatcher, SimpleTableSerde as SimpleTable,
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
};

fn decode_text(env: &Env<'_>, text_bytes: JByteArray<'_>) -> JniResult<String> {
    String::from_utf8(env.convert_byte_array(text_bytes)?)
        .map_err(|error| JniError::ParseFailed(error.to_string()))
}

fn parse_simple_table(simple_table_bytes: &[u8]) -> JniResult<SimpleTable<'_>> {
    sonic_rs::from_slice(simple_table_bytes)
        .map_err(|error| JniError::ParseFailed(error.to_string()))
}

fn serialize_results(results: &[matcher_rs::SimpleResult<'_>]) -> JniResult<String> {
    sonic_rs::to_string(results).map_err(|error| JniError::ParseFailed(error.to_string()))
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
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherProcessAsString<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    text_bytes: JByteArray<'local>,
) -> jstring {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(ptr::null_mut());
        };

        let text = decode_text(env, text_bytes)?;
        let results = matcher.process(&text);
        let json = serialize_results(&results)?;

        Ok(env.new_string(json)?.into_raw())
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
pub extern "system" fn Java_com_matcherjava_MatcherJava_simpleMatcherBatchProcessAsString<
    'local,
>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    matcher_ptr: jlong,
    texts_bytes: JObjectArray<'local>,
) -> jstring {
    env.with_env(|env| -> JniResult<_> {
        let Some(matcher) = matcher_from_ptr(matcher_ptr) else {
            return Ok(ptr::null_mut());
        };

        let texts = decode_texts(env, &texts_bytes)?;
        let all_results: Vec<Vec<matcher_rs::SimpleResult>> =
            texts.iter().map(|t| matcher.process(t)).collect();
        let json =
            sonic_rs::to_string(&all_results).map_err(|e| JniError::ParseFailed(e.to_string()))?;

        Ok(env.new_string(json)?.into_raw())
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
