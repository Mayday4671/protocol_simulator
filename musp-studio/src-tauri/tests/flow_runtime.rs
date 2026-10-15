use musp_studio::runtime::*;
use serde_json::{Value, json};

fn publish(engine: &mut FlowEngine, definition: Value) {
    engine
        .publish(serde_json::from_value(definition).unwrap())
        .unwrap();
}
fn waiting_flow(id: &str, retries: u32) -> Value {
    json!({"id":id,"nodes":[
      {"id":"start","type":"START"},
      {"id":"send","type":"SEND_AND_WAIT","data":{"code":1,"expectedCodes":[2],"timeoutMs":100,"retries":retries,"retryIntervalMs":20,"values":{"srcAddr":[0,0,59],"dstAddr":[0,0,1],"sessionId":0}}},
      {"id":"end","type":"END"},{"id":"error","type":"ERROR"}],
      "edges":[{"source":"start","target":"send"},{"source":"send","target":"end","label":"matched"},{"source":"send","target":"error","label":"timeout"},{"source":"send","target":"error","label":"error"}]})
}
fn context() -> Value {
    json!({"endpointId":"udp","protocolVersionId":"v1","remote":"127.0.0.1:9000"})
}
fn start_wait(engine: &mut FlowEngine, retries: u32) -> (String, SendAction) {
    publish(engine, waiting_flow("wait", retries));
    let run = engine.start("wait", context(), 0).unwrap();
    let actions = engine.poll(0);
    let send = actions
        .into_iter()
        .find_map(|a| {
            if let FlowAction::Send(s) = a {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    (run, send)
}
fn response(send: &SendAction) -> PacketMeta {
    PacketMeta {
        endpoint_id: send.endpoint_id.clone(),
        protocol_version_id: send.protocol_version_id.clone(),
        remote: send.remote.clone(),
        msg_code: 2,
        src_addr: vec![0, 0, 1],
        dst_addr: vec![0, 0, 59],
        transaction_id: send.values["transactionId"].as_u64().map(|v| v as u32),
        session_id: Some(0x1234),
        valid: true,
        payload: json!({"body":{"resultCode":0}}),
        ..Default::default()
    }
}

#[test]
fn pending_exists_before_first_send_and_fast_reply_completes_once() {
    let mut e = FlowEngine::default();
    let (run, send) = start_wait(&mut e, 0);
    assert_eq!(
        e.pending.requests[send.pending_id.as_ref().unwrap()].status,
        PendingStatus::Waiting
    );
    let (outcome, actions) = e.on_packet(&response(&send), 1);
    assert!(matches!(outcome, MatchOutcome::Matched(_)));
    assert!(actions.iter().any(|a| matches!(
        a,
        FlowAction::Finished {
            status: FlowStatus::Completed,
            ..
        }
    )));
    assert!(
        e.complete_action(&send.action_id, Ok(Value::Null), 2)
            .is_empty()
    );
    assert!(matches!(
        e.on_packet(&response(&send), 3).0,
        MatchOutcome::Duplicate(_)
    ));
    assert_eq!(e.runs[&run].status, FlowStatus::Completed);
}

#[test]
fn retries_obey_interval_reuse_transaction_and_timeout_branch() {
    let mut e = FlowEngine::default();
    let (run, send) = start_wait(&mut e, 1);
    e.complete_action(&send.action_id, Ok(Value::Null), 0);
    assert!(e.poll(100).is_empty());
    assert!(e.poll(119).is_empty());
    let actions = e.poll(120);
    let retry = actions
        .into_iter()
        .find_map(|a| {
            if let FlowAction::Send(s) = a {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(retry.attempt, 2);
    assert_eq!(retry.values["transactionId"], send.values["transactionId"]);
    e.complete_action(&retry.action_id, Ok(Value::Null), 120);
    let actions = e.poll(220);
    assert!(actions.iter().any(|a| matches!(
        a,
        FlowAction::Finished {
            status: FlowStatus::Failed,
            ..
        }
    )));
    assert_eq!(e.runs[&run].status, FlowStatus::Failed);
    assert!(matches!(
        e.on_packet(&response(&send), 221).0,
        MatchOutcome::Late(_)
    ));
}

#[test]
fn cancellation_removes_pending_and_prevents_retry_or_late_completion() {
    let mut e = FlowEngine::default();
    let (run, send) = start_wait(&mut e, 3);
    e.cancel(&run, 1);
    assert!(e.poll(10_000).is_empty());
    assert!(
        e.complete_action(&send.action_id, Ok(Value::Null), 2)
            .is_empty()
    );
    assert_eq!(
        e.pending.requests[send.pending_id.as_ref().unwrap()].status,
        PendingStatus::Cancelled
    );
    assert!(matches!(
        e.on_packet(&response(&send), 3).0,
        MatchOutcome::Late(_)
    ));
    assert_eq!(e.runs[&run].status, FlowStatus::Cancelled);
}

#[test]
fn delayed_loop_runs_exactly_three_times_and_respects_pause() {
    let mut e = FlowEngine::default();
    publish(
        &mut e,
        json!({"id":"loop","nodes":[{"id":"s","type":"START"},{"id":"l","type":"LOOP","data":{"maxIterations":3,"count":3}},
       {"id":"a","type":"ASSIGN","data":{"path":"lastIndex","value":"${step.index}"}},{"id":"d","type":"DELAY","data":{"durationMs":10}},{"id":"end","type":"END"}],
       "edges":[{"source":"s","target":"l"},{"source":"l","target":"a","label":"body"},{"source":"l","target":"end","label":"done"},{"source":"a","target":"d"},{"source":"d","target":"l"}]}),
    );
    let run = e.start("loop", json!({}), 0).unwrap();
    e.poll(0);
    assert_eq!(e.runs[&run].variables["lastIndex"], 0);
    e.poll(9);
    assert_eq!(e.runs[&run].variables["lastIndex"], 0);
    e.pause(&run).unwrap();
    e.poll(10);
    assert_eq!(e.runs[&run].variables["lastIndex"], 0);
    e.resume(&run, 20);
    assert_eq!(e.runs[&run].variables["lastIndex"], 1);
    e.poll(30);
    assert_eq!(e.runs[&run].variables["lastIndex"], 2);
    e.poll(40);
    assert_eq!(e.runs[&run].status, FlowStatus::Completed);
}

#[test]
fn send_failure_cancels_wait_and_runs_error_branch() {
    let mut e = FlowEngine::default();
    let (run, send) = start_wait(&mut e, 2);
    let actions = e.complete_action(&send.action_id, Err("socket not bound".into()), 1);
    assert!(actions.iter().any(|a| matches!(
        a,
        FlowAction::Finished {
            status: FlowStatus::Failed,
            ..
        }
    )));
    assert_eq!(
        e.pending.requests[send.pending_id.as_ref().unwrap()].status,
        PendingStatus::SendFailed
    );
    assert_eq!(e.runs[&run].status, FlowStatus::Failed);
    assert!(e.poll(1000).is_empty());
}

#[test]
fn subflow_failed_end_propagates_instead_of_returning_success() {
    let mut e = FlowEngine::default();
    publish(
        &mut e,
        json!({"id":"child","nodes":[{"id":"s","type":"START"},{"id":"e","type":"END","data":{"result":"FAILED"}}],"edges":[{"source":"s","target":"e"}]}),
    );
    publish(
        &mut e,
        json!({"id":"parent","nodes":[{"id":"s","type":"START"},{"id":"sub","type":"INVOKE_SUBFLOW","data":{"flowId":"child"}},{"id":"e","type":"END"}],"edges":[{"source":"s","target":"sub"},{"source":"sub","target":"e"}]}),
    );
    let run = e.start("parent", json!({}), 0).unwrap();
    e.poll(1);
    assert_eq!(e.runs[&run].status, FlowStatus::Failed);
}

#[test]
fn graph_publish_rejects_missing_next_and_ambiguous_edges() {
    let mut e = FlowEngine::default();
    let missing =
        json!({"id":"bad","nodes":[{"id":"s","type":"START"},{"id":"e","type":"END"}],"edges":[]});
    assert!(e.publish(serde_json::from_value(missing).unwrap()).is_err());
    let ambiguous = json!({"id":"bad","nodes":[{"id":"s","type":"START"},{"id":"e","type":"END"}],"edges":[{"source":"s","target":"e"},{"source":"s","target":"e"}]});
    assert!(
        e.publish(serde_json::from_value(ambiguous).unwrap())
            .is_err()
    );
}

#[test]
fn boolean_conditions_support_parentheses_and_quoted_operator_text() {
    let mut e = FlowEngine::default();
    publish(
        &mut e,
        json!({"id":"conditions","nodes":[{"id":"s","type":"START"},{"id":"c","type":"CONDITION","data":{"expression":"(a == 1 || b == 2) && label == \"x==y\""}},{"id":"e","type":"END"},{"id":"bad","type":"ERROR"}],"edges":[{"source":"s","target":"c"},{"source":"c","target":"e","label":"true"},{"source":"c","target":"bad","label":"false"}]}),
    );
    let run = e
        .start("conditions", json!({"a":0,"b":2,"label":"x==y"}), 0)
        .unwrap();
    e.poll(1);
    assert_eq!(e.runs[&run].status, FlowStatus::Completed);
}

#[test]
fn late_reply_is_rejected_even_before_scheduler_tick() {
    let mut e = FlowEngine::default();
    let (run, send) = start_wait(&mut e, 0);
    assert!(matches!(
        e.on_packet(&response(&send), 101).0,
        MatchOutcome::Late(_)
    ));
    e.poll(101);
    assert_eq!(e.runs[&run].status, FlowStatus::Failed);
}

#[test]
fn only_reachable_subflow_references_affect_start() {
    let mut e = FlowEngine::default();
    publish(&mut e, waiting_flow("valid", 0));
    publish(
        &mut e,
        json!({"id":"unrelated","nodes":[{"id":"s","type":"START"},{"id":"sub","type":"INVOKE_SUBFLOW","data":{"flowId":"missing"}},{"id":"e","type":"END"}],"edges":[{"source":"s","target":"sub"},{"source":"sub","target":"e"}]}),
    );
    assert!(e.start("valid", context(), 0).is_ok());
    assert!(e.start("unrelated", context(), 0).is_err());
}

#[test]
fn total_duration_cancels_due_retry_before_it_escapes_engine() {
    let mut e = FlowEngine::default();
    let mut flow = waiting_flow("duration", 1);
    flow["config"] = json!({"maxDurationMs":150});
    publish(&mut e, flow);
    let run = e.start("duration", context(), 0).unwrap();
    e.poll(0);
    e.poll(100);
    let actions = e.poll(150);
    assert!(!actions.iter().any(|a| matches!(a, FlowAction::Send(_))));
    assert_eq!(e.runs[&run].status, FlowStatus::Failed);
}

#[test]
fn received_auth_event_before_wait_is_retained_without_lost_wakeup() {
    let mut e = FlowEngine::default();
    publish(
        &mut e,
        json!({"id":"auth","nodes":[{"id":"s","type":"START"},{"id":"d","type":"DELAY","data":{"durationMs":10}},
        {"id":"w","type":"WAIT_EVENT","data":{"event":"authReady","timeoutMs":50}},{"id":"a","type":"ASSIGN","data":{"path":"auth","value":"${step}"}},
        {"id":"e","type":"END"},{"id":"bad","type":"ERROR"}],"edges":[{"source":"s","target":"d"},{"source":"d","target":"w"},{"source":"w","target":"a","label":"event"},{"source":"w","target":"bad","label":"timeout"},{"source":"a","target":"e"}]}),
    );
    let run = e.start("auth", json!({}), 0).unwrap();
    e.poll(0);
    e.emit_event(&run, "authReady", json!({"sessionId":1234}), 1);
    e.poll(10);
    assert_eq!(e.runs[&run].status, FlowStatus::Completed);
    assert_eq!(e.runs[&run].variables["auth"]["sessionId"], 1234);
}

#[test]
fn running_flow_uses_immutable_graph_snapshot_after_republish() {
    let mut e = FlowEngine::default();
    let flow = json!({"id":"snapshot","version":"1","nodes":[{"id":"s","type":"START"},{"id":"d","type":"DELAY","data":{"durationMs":10}},{"id":"a","type":"ASSIGN","data":{"path":"version","value":1}},{"id":"e","type":"END"}],"edges":[{"source":"s","target":"d"},{"source":"d","target":"a"},{"source":"a","target":"e"}]});
    publish(&mut e, flow.clone());
    let run = e.start("snapshot", json!({}), 0).unwrap();
    e.poll(0);
    let mut changed = flow;
    changed["nodes"][2]["data"]["value"] = json!(2);
    changed["version"] = json!("2");
    publish(&mut e, changed);
    e.poll(10);
    assert_eq!(e.runs[&run].variables["version"], 1);
    assert_eq!(e.runs[&run].definition_version, "1");
}

#[test]
fn host_business_failure_overrides_just_completed_run_and_blocks_followups() {
    let mut e = FlowEngine::default();
    let (run, send) = start_wait(&mut e, 0);
    e.on_packet(&response(&send), 1);
    assert_eq!(e.runs[&run].status, FlowStatus::Completed);
    let actions = e.fail_run(&run, "business snapshot rejected".into(), 2);
    assert!(matches!(
        &actions[0],
        FlowAction::Finished {
            status: FlowStatus::Failed,
            ..
        }
    ));
    assert_eq!(
        e.runs[&run].failure.as_deref(),
        Some("business snapshot rejected")
    );
    assert!(e.poll(300).is_empty());
}

#[test]
fn malformed_expression_is_rejected_before_publication() {
    let mut e = FlowEngine::default();
    let flow = json!({"id":"bad-expression","nodes":[{"id":"s","type":"START"},{"id":"c","type":"CONDITION","data":{"expression":"fetch('remote')"}},{"id":"e","type":"END"},{"id":"bad","type":"ERROR"}],"edges":[{"source":"s","target":"c"},{"source":"c","target":"e","label":"true"},{"source":"c","target":"bad","label":"false"}]});
    assert!(e.publish(serde_json::from_value(flow).unwrap()).is_err());
}

#[test]
fn concurrent_runs_do_not_share_transactions_or_response_variables() {
    let mut e = FlowEngine::default();
    publish(&mut e, waiting_flow("concurrent", 0));
    let a = e.start("concurrent", context(), 0).unwrap();
    let b = e.start("concurrent", context(), 0).unwrap();
    let sends: Vec<_> = e
        .poll(0)
        .into_iter()
        .filter_map(|action| {
            if let FlowAction::Send(s) = action {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(sends.len(), 2);
    assert_ne!(
        sends[0].values["transactionId"],
        sends[1].values["transactionId"]
    );
    let for_b = sends.iter().find(|s| s.run_id == b).unwrap();
    e.on_packet(&response(for_b), 1);
    assert_eq!(e.runs[&b].status, FlowStatus::Completed);
    assert_eq!(e.runs[&a].status, FlowStatus::Waiting);
    let for_a = sends.iter().find(|s| s.run_id == a).unwrap();
    e.on_packet(&response(for_a), 2);
    assert_eq!(e.runs[&a].status, FlowStatus::Completed);
}

#[test]
fn trace_retention_is_bounded_and_dropped_events_are_counted() {
    let mut e = FlowEngine::default();
    publish(
        &mut e,
        json!({"id":"trace","nodes":[{"id":"s","type":"START"},{"id":"l","type":"LOOP","data":{"count":6000,"maxIterations":6000}},{"id":"a","type":"ASSIGN","data":{"path":"last","value":"${step.index}"}},{"id":"e","type":"END"}],"edges":[{"source":"s","target":"l"},{"source":"l","target":"a","label":"body"},{"source":"a","target":"l"},{"source":"l","target":"e","label":"done"}]}),
    );
    let run = e.start("trace", json!({}), 0).unwrap();
    for _ in 0..100 {
        e.poll(1);
        if e.runs[&run].status == FlowStatus::Completed {
            break;
        }
    }
    assert_eq!(e.runs[&run].status, FlowStatus::Completed);
    assert_eq!(e.runs[&run].trace.len(), 10_000);
    assert!(e.runs[&run].trace_dropped > 0);
    assert_eq!(
        e.runs[&run].trace.len() as u64 + e.runs[&run].trace_dropped,
        e.runs[&run].steps
    );
}
