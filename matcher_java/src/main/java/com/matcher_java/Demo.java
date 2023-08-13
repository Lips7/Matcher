package com.matcher_java;

import org.msgpack.core.MessageBufferPacker;
import org.msgpack.core.MessagePack;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;

import java.io.IOException;

interface Matcher extends Library {
    Matcher INSTANCE = (Matcher) Native.load(
            Matcher.class.getResource("/matcher_c.so").getPath(),
            Matcher.class);

    Pointer init_matcher(byte[] match_table_dict_bytes);

    boolean matcher_is_match(Pointer matcher, byte[] text_bytes);

    Pointer matcher_word_match(Pointer matcher, byte[] text_bytes);

    void drop_matcher(Pointer matcher);

    Pointer init_simple_matcher(byte[] simple_wordlist_dict_bytes);

    boolean simple_matcher_is_match(Pointer simple_matcher, byte[] text_bytes);

    Pointer simple_matcher_process(Pointer simple_matcher, byte[] text_bytes);

    void drop_simple_matcher(Pointer simple_matcher);

    void drop_string(Pointer ptr);
}

public class Demo {
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
        packer.packInt(15);
        packer.packArrayHeader(1);
        packer.packMapHeader(2);
        packer.packString("word_id");
        packer.packInt(1);
        packer.packString("word");
        packer.packString("你好");
        packer.close();

        byte[] simple_wordlist_dict_bytes = packer.toByteArray();

        Matcher instance = Matcher.INSTANCE;

        Pointer simple_matcher = instance.init_simple_matcher(simple_wordlist_dict_bytes);

        byte[] str_bytes = "你好".getBytes("utf-8");
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
        packer.packString("test");
        packer.packArrayHeader(1);
        packer.packMapHeader(5);
        packer.packString("table_id");
        packer.packInt(1);
        packer.packString("match_table_type");
        packer.packString("simple");
        packer.packString("wordlist");
        packer.packArrayHeader(1);
        packer.packString("你好");
        packer.packString("exemption_wordlist");
        packer.packArrayHeader(0);
        packer.packString("simple_match_type");
        packer.packInt(15);
        packer.close();

        byte[] table_match_dict_bytes = packer.toByteArray();

        Matcher instance = Matcher.INSTANCE;

        Pointer matcher = instance.init_matcher(table_match_dict_bytes);

        byte[] str_bytes = "你好".getBytes("utf-8");
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
