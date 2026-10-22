import type { FlowRow, FlowTrace, FlowTraceEvent } from "@/lib/api";

export interface TimelinePhase {
  key: string;
  stepId: string;
  phase: "send" | "response" | "receive";
  name: string;
  protocolId: string;
  state: string;
  last?: FlowTraceEvent;
  events: FlowTraceEvent[];
}
const working = ["running", "waiting", "waiting_send", "delay"];

// Expand a correlated exchange into its two observable actions. Keep the engine's
// transaction intact so immediate ACKs and request/response correlation still work.
export function timelinePhases(flow: FlowRow, trace?: FlowTrace): TimelinePhase[] {
  return flow.steps.flatMap<TimelinePhase>(step => {
    const events = (trace?.events ?? []).filter(e => e.step_id === step.id && e.status !== "transition");
    const last = trace?.step_states[step.id];
    let state = last?.status ?? "pending";
    if (trace?.ended_at && working.includes(state)) state = trace.status === "aborted" ? "aborted" : "error";
    const base = {stepId:step.id, name:step.name, last};
    if (step.action === "receive" || !step.expect_protocol) {
      const phase = step.action === "receive" ? "receive" : "send";
      return [{...base, key:`${step.id}:${phase}`, phase, protocolId:step.protocol_id, state, events}];
    }
    const sendEvents: FlowTraceEvent[] = [], responseEvents: FlowTraceEvent[] = [];
    const sentAttempts = new Set<number>();
    for (const event of events) {
      if (event.status === "sent" || event.packet?.direction === "out") {
        sentAttempts.add(event.attempt); sendEvents.push(event);
      } else if (event.status === "waiting" || (event.packet?.direction === "in" && event.status !== "unmatched")) {
        sentAttempts.add(event.attempt); responseEvents.push(event);
      } else if (event.packet?.direction === "in") {
        responseEvents.push(event);
      } else if (sentAttempts.has(event.attempt)) responseEvents.push(event);
      else sendEvents.push(event);
    }
    const attempt = last?.attempt ?? 0;
    const sent = sentAttempts.has(attempt) || ["waiting", "success"].includes(last?.status ?? "");
    const sendLast = sendEvents.filter(e => e.attempt === attempt).slice(-1)[0];
    const responseLast = responseEvents.filter(e => e.attempt === attempt).slice(-1)[0];
    return [
      {...base, key:`${step.id}:send`, phase:"send", protocolId:step.protocol_id,
        state:sent ? "success" : state, last:sendLast ?? (!sent ? last : undefined), events:sendEvents},
      {...base, key:`${step.id}:response`, phase:"response", protocolId:step.expect_protocol,
        state:sent ? (["sent","running","waiting_send"].includes(state) ? "waiting" : state) : "pending",
        last:sent ? (responseLast ?? last) : undefined, events:responseEvents},
    ];
  });
}

export const timelinePhaseCount = (flow: FlowRow) => flow.steps.reduce((n,s) => n + (s.action !== "receive" && s.expect_protocol ? 2 : 1), 0);
