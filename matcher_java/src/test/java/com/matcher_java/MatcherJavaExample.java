package com.matcher_java;

import com.sun.jna.Pointer;
import java.io.IOException;
import org.msgpack.core.MessageBufferPacker;
import org.msgpack.core.MessagePack;

public class MatcherJavaExample {
    public static void main(String[] args) throws IOException {
        System.out.println("Simple Matcher Test");
        simple_matcher_process_demo();

        System.out.println("\n");

        System.out.println("Matcher Test");
        matcher_process_demo();
    }

    public static void simple_matcher_process_demo() throws IOException {
        MessageBufferPacker packer = MessagePack.newDefaultBufferPacker();
        packer.packMapHeader(1);
        packer.packInt(30); // 30 = FanjianDeleteNormalize
        packer.packMapHeader(1);
        packer.packInt(1);
        packer.packString("hello&world");
        packer.close();

        byte[] smt_word_map_bytes = packer.toByteArray();

        MatcherJava instance = MatcherJava.INSTANCE;

        Pointer simple_matcher = instance.init_simple_matcher(smt_word_map_bytes);

        byte[] str_bytes = "hello,world".getBytes("utf-8");
        byte[] c_str_bytes = new byte[str_bytes.length + 1];
        System.arraycopy(str_bytes, 0, c_str_bytes, 0, str_bytes.length);

        boolean is_match = instance.simple_matcher_is_match(simple_matcher, c_str_bytes);
        System.out.printf("is_match: %s\n", is_match);

        Pointer match_res_ptr = instance.simple_matcher_process(simple_matcher, c_str_bytes);
        String match_res = match_res_ptr.getString(0, "utf-8");
        System.out.printf("match_res: %s\n", match_res);
        instance.drop_string(match_res_ptr);

        instance.drop_simple_matcher(simple_matcher);
    }

    public static void matcher_process_demo() throws IOException {
        MessageBufferPacker packer = MessagePack.newDefaultBufferPacker();

        packer.packMapHeader(1);
        packer.packInt(1);
        packer.packArrayHeader(1);
        packer.packMapHeader(5);
        packer.packString("table_id");
        packer.packInt(1);
        packer.packString("match_table_type");
        packer.packMapHeader(1);
        packer.packString("simple_match_type");
        packer.packInt(30); // 30 = FanjianDeleteNormalize
        packer.packString("word_list");
        packer.packArrayHeader(1);
        packer.packString("hello&world");
        packer.packString("exemption_simple_match_type");
        packer.packInt(1); // 1 = None
        packer.packString("exemption_word_list");
        packer.packArrayHeader(0);
        byte[] match_table_map_dict_bytes = packer.toByteArray();
        packer.close();

        MatcherJava instance = MatcherJava.INSTANCE;

        Pointer matcher = instance.init_matcher(match_table_map_dict_bytes);

        byte[] str_bytes = "hello,world".getBytes("utf-8");
        byte[] c_str_bytes = new byte[str_bytes.length + 1];
        System.arraycopy(str_bytes, 0, c_str_bytes, 0, str_bytes.length);

        boolean is_match = instance.matcher_is_match(matcher, c_str_bytes);
        System.out.printf("is_match: %s\n", is_match);

        Pointer match_res_ptr = instance.matcher_word_match(matcher, c_str_bytes);
        String match_res = match_res_ptr.getString(0, "utf-8");
        System.out.printf("match_res: %s\n", match_res);
        instance.drop_string(match_res_ptr);

        instance.drop_matcher(matcher);
    }
}