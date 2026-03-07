package com.matcherjava.extensiontypes;

import com.alibaba.fastjson.serializer.JSONSerializer;
import com.alibaba.fastjson.serializer.ObjectSerializer;
import java.io.IOException;
import java.lang.reflect.Type;

/**
 * Serializer for ProcessType.
 */
public class ProcessTypeSerializer implements ObjectSerializer {
  @Override
  public void write(
      JSONSerializer serializer, Object object, Object fieldName, Type fieldType, int features)
      throws IOException {
    ProcessType processType = (ProcessType) object;
    if (fieldName != null) {
      serializer.write(processType.getValue());
    } else {
      serializer.write(processType.toString());
    }
  }
}