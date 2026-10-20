-- 010 彻底移除正则匹配: 统一改用字段条件(match_fields_json)
UPDATE protocols SET match_rule = NULL;
