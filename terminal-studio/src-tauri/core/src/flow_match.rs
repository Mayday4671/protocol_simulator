use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::{error::{AppError, AppResult}, models::{FieldRole, ProtocolField}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPair { pub request: String, pub response: String }

pub fn pairs(request: &[ProtocolField], response: &[ProtocolField], configured: &[FieldPair], allow_unkeyed: bool) -> AppResult<Vec<FieldPair>> {
    let mut out = configured.to_vec();
    if out.is_empty() {
        for (qr, ar) in [(FieldRole::Correlation, FieldRole::Correlation), (FieldRole::SrcId, FieldRole::DstId), (FieldRole::DstId, FieldRole::SrcId)] {
            let q: Vec<_> = request.iter().filter(|f| f.role == qr).collect();
            let a: Vec<_> = response.iter().filter(|f| f.role == ar).collect();
            if q.len() > 1 || a.len() > 1 { return Err(AppError::msg("关联字段不唯一，请在步骤中明确选择请求与应答字段")); }
            if let (Some(q), Some(a)) = (q.first(), a.first()) {
                out.push(FieldPair { request: q.name.clone(), response: a.name.clone() });
            } else if qr == FieldRole::Correlation && !allow_unkeyed {
                return Err(AppError::msg("未找到请求/应答关联序号，请配置关联字段；无序号协议需勾选串行匹配"));
            }
        }
    }
    for pair in &out {
        let q: Vec<_> = request.iter().filter(|f| f.name == pair.request).collect();
        let a: Vec<_> = response.iter().filter(|f| f.name == pair.response).collect();
        if q.len() != 1 || a.len() != 1 || !q[0].unit_fields.is_empty() || !a[0].unit_fields.is_empty() {
            return Err(AppError::msg("关联字段必须是唯一的单值字段"));
        }
        if q[0].length != a[0].length { return Err(AppError::msg("请求/应答关联字段字节数不一致")); }
    }
    Ok(out)
}
pub fn matches(pairs: &[FieldPair], request: &HashMap<String, String>, response: &HashMap<String, String>) -> bool {
    pairs.iter().all(|p| match (request.get(&p.request), response.get(&p.response)) {
        (Some(q), Some(a)) => !q.is_empty() && q == a,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(name: &str, role: &str) -> ProtocolField {
        serde_json::from_value(serde_json::json!({"name":name,"len":1,"enc":"hex","role":role})).unwrap()
    }
    #[test]
    fn automatic_sequence_and_reverse_addresses() {
        let q = vec![field("流水", "correlation"), field("发送方", "src_id"), field("接收方", "dst_id")];
        let a = vec![field("回显", "correlation"), field("源", "src_id"), field("目的", "dst_id")];
        let keys = pairs(&q, &a, &[], false).unwrap();
        assert_eq!(keys.len(), 3);
        let req = HashMap::from([("流水".into(), "01".into()), ("发送方".into(), "0A".into()), ("接收方".into(), "0B".into())]);
        let mut ack = HashMap::from([("回显".into(), "01".into()), ("源".into(), "0B".into()), ("目的".into(), "0A".into())]);
        assert!(matches(&keys, &req, &ack));
        ack.insert("回显".into(), "02".into());
        assert!(!matches(&keys, &req, &ack));
        ack.insert("回显".into(), "01".into()); ack.insert("源".into(), "0C".into());
        assert!(!matches(&keys, &req, &ack));
    }
    #[test]
    fn missing_identity_requires_explicit_serial_choice() {
        assert!(pairs(&[], &[], &[], false).is_err());
        assert!(pairs(&[], &[], &[], true).unwrap().is_empty());
        assert!(pairs(&[field("a", "none")], &[field("b", "none")], &[FieldPair {request:"missing".into(),response:"b".into()}], false).is_err());
    }
    #[test]
    fn explicit_pairs_never_match_missing_values() {
        let keys = vec![FieldPair { request:"q".into(), response:"a".into() }];
        assert!(!matches(&keys, &HashMap::new(), &HashMap::new()));
        let q = HashMap::from([("q".into(), "0001".into())]);
        assert!(!matches(&keys, &q, &HashMap::from([("a".into(), "01".into())])));
        assert!(matches(&keys, &q, &HashMap::from([("a".into(), "0001".into())])));
    }
}
