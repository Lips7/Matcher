package com.matcher_java.extension_types;

import java.io.IOException;
import java.lang.reflect.Type;

import com.alibaba.fastjson.serializer.JSONSerializer;
import com.alibaba.fastjson.serializer.ObjectSerializer;

public class ProcessTypeSerializer implements ObjectSerializer {
    @Override
    public void write(JSONSerializer serializer, Object object, Object fieldName, Type fieldType, int features)
            throws IOException {
        ProcessType processType = (ProcessType) object;
        if (fieldName != null) {
            serializer.write(processType.getValue());
        } else {
            serializer.write(processType.toString());
        }
    }
}