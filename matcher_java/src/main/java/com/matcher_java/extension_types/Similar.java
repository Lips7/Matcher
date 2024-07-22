package com.matcher_java.extension_types;

import com.alibaba.fastjson.PropertyNamingStrategy;
import com.alibaba.fastjson.annotation.JSONType;

@JSONType(naming = PropertyNamingStrategy.SnakeCase)
public class Similar {
    private ProcessType process_type;
    private SimMatchType sim_match_type;
    private float threshold;

    public Similar(ProcessType process_type, SimMatchType sim_match_type, float threshold) {
        this.process_type = process_type;
        this.sim_match_type = sim_match_type;
        this.threshold = threshold;
    }

    public ProcessType getProcessType() {
        return process_type;
    }

    public SimMatchType getSimMatchType() {
        return sim_match_type;
    }

    public float getThreshold() {
        return threshold;
    }
}