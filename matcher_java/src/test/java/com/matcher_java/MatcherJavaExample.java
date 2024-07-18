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
        packer.packInt(1); // 1 = None
        packer.packMapHeader(1);
        packer.packInt(1);
        packer.packString("hello&world");
        packer.close();

        byte[] simple_table_bytes = packer.toByteArray();

        MatcherJava instance = MatcherJava.INSTANCE;

        Pointer simple_matcher = instance.init_simple_matcher(simple_table_bytes);

        byte[] str_bytes = "hello,world".getBytes("utf-8");
        byte[] c_str_bytes = new byte[str_bytes.length + 1];
        System.arraycopy(str_bytes, 0, c_str_bytes, 0, str_bytes.length);

        boolean is_match = instance.simple_matcher_is_match(simple_matcher, c_str_bytes);
        System.out.printf("is_match: %s\n", is_match);

        Pointer match_res_ptr = instance.simple_matcher_process_as_string(simple_matcher, c_str_bytes);
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
        packer.packString("process_type");
        packer.packInt(1); // 1 = None
        packer.packString("word_list");
        packer.packArrayHeader(1);
        packer.packString("hello&world");
        packer.packString("exemption_process_type");
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

        Pointer match_res_ptr_1 = instance.matcher_process_as_string(matcher, c_str_bytes);
        String match_res_1 = match_res_ptr_1.getString(0, "utf-8");
        System.out.printf("match_res: %s\n", match_res_1);
        instance.drop_string(match_res_ptr_1);

        Pointer match_res_ptr_2 = instance.matcher_word_match_as_string(matcher, c_str_bytes);
        String match_res_2 = match_res_ptr_2.getString(0, "utf-8");
        System.out.printf("match_res: %s\n", match_res_2);
        instance.drop_string(match_res_ptr_2);

        instance.drop_matcher(matcher);
    }
}