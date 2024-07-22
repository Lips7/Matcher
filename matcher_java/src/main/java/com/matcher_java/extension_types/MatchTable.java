package com.matcher_java.extension_types;

import java.util.List;
import java.util.Map;

import com.alibaba.fastjson.PropertyNamingStrategy;
import com.alibaba.fastjson.annotation.JSONType;

@JSONType(naming = PropertyNamingStrategy.SnakeCase)
public class MatchTable {
    private int table_id;
    private Map<String, ?> match_table_type;
    private List<String> word_List;
    private ProcessType exemption_process_type;
    private List<String> exemption_word_list;

    public MatchTable(int table_id, Map<String, ?> match_table_type, List<String> word_List,
            ProcessType exemption_process_type, List<String> exemption_word_list) {
        this.table_id = table_id;
        this.match_table_type = match_table_type;
        this.word_List = word_List;
        this.exemption_process_type = exemption_process_type;
        this.exemption_word_list = exemption_word_list;
    }

    public int getTableId() {
        return table_id;
    }

    public void setTableId(int table_id) {
        this.table_id = table_id;
    }

    public Map<String, ?> getMatchTableType() {
        return match_table_type;
    }

    public void setMatchTableType(Map<String, ?> match_table_type) {
        this.match_table_type = match_table_type;
    }

    public List<String> getWordList() {
        return word_List;
    }

    public void setWordList(List<String> word_List) {
        this.word_List = word_List;
    }

    public ProcessType getExemptionProcessType() {
        return exemption_process_type;
    }

    public void setExemptionProcessType(ProcessType exemption_process_type) {
        this.exemption_process_type = exemption_process_type;
    }

    public List<String> getExemptionWordList() {
        return exemption_word_list;
    }

    public void setExemptionWordList(List<String> exemption_word_list) {
        this.exemption_word_list = exemption_word_list;
    }
}
