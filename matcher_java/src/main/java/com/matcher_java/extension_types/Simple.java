package com.matcher_java.extension_types;

import com.alibaba.fastjson.PropertyNamingStrategy;
import com.alibaba.fastjson.annotation.JSONType;

@JSONType(naming = PropertyNamingStrategy.SnakeCase)
public class Simple {
    private ProcessType process_type;

    public Simple(ProcessType process_type) {
        this.process_type = process_type;
    }

    public ProcessType getProcessType() {
        return process_type;
    }
}