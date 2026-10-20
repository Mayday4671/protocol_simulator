//! Validate the IoT fixture with the real engine and an isolated localhost UDP peer.
//!
//! Run from src-tauri:
//! cargo run --features e2e --bin e2e_iot_dataset -- <project-absolute-path>/artifacts/iot-dataset-validation.db
//! The explicit fixture must be inside this project's artifacts directory. APPDATA,
//! relative paths, missing files, and production databases are refused before opening.
//! Only the fixture is writable; test nodes, profiles, and logs stay in that fixture.

use simterm_core::fields::{extract_field_values, is_variable_len, parse_fields, req_ref_name};
use simterm_core::models::{
    DefMode, FieldEnc, FieldRole, MatchCond, Node, PayloadFormat, Protocol, ProtocolField,
};
use simterm_core::render::{compact_hex, DictMap};
use simterm_core::rules::{check_match_conds, effective_conds};
use simulated_terminal_lib::engine::{state::EngineState, udp};
use simulated_terminal_lib::{db, e2e_validate_all_protocols};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tokio::net::UdpSocket;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Values = HashMap<String, String>;

const TERMINAL_A: &str = "01000001";
const TERMINAL_B: &str = "01000002";
const TERMINAL_C: &str = "01000003";
const FAMILIES: [(&str, &str); 3] = [
    ("iot2_env", "E101"),
    ("iot2_access", "D201"),
    ("iot2_gateway", "F301"),
];
const OPERATIONS: [(&str, &str, &str); 9] = [
    ("register", "接入管理", "01"),
    ("unregister", "接入管理", "02"),
    ("heartbeat", "接入管理", "03"),
    ("read", "参数配置", "11"),
    ("write", "参数配置", "12"),
    ("topology", "参数配置", "13"),
    ("realtime", "业务上报", "21"),
    ("alarm", "业务上报", "22"),
    ("batch", "业务上报", "23"),
];

macro_rules! require {
    ($condition:expr, $($message:tt)+) => {
        if !$condition { return Err(format!($($message)+).into()); }
    };
}

struct ProtocolContext {
    protocol: Protocol,
    fields: Vec<ProtocolField>,
    dicts: DictMap,
    conds: Vec<MatchCond>,
}

struct ExpectedField {
    index: usize,
    length: usize,
    bytes: Option<Vec<u8>>,
    list_count: Option<usize>,
}

struct Prepared {
    params: Values,
    expected: Vec<ExpectedField>,
}

#[derive(Default)]
struct Coverage {
    frames: usize,
    branches: usize,
    populated_lists: usize,
    empty_lists: usize,
    automatic_pairs: usize,
    negative_matches: usize,
}

fn fixture_path() -> TestResult<PathBuf> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err("supply exactly one absolute path to the project artifacts fixture".into());
    }
    let requested = PathBuf::from(&args[0]);
    if !requested.is_absolute() {
        return Err("relative database paths are refused".into());
    }
    let path = requested.canonicalize().map_err(|e| {
        format!(
            "fixture does not exist or cannot be resolved: {}: {e}",
            requested.display()
        )
    })?;
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("manifest has no project parent")?
        .join("artifacts")
        .canonicalize()?;
    if !path.starts_with(&artifacts) || !path.is_file() {
        return Err(
            "database must be an existing file inside this project's artifacts directory".into(),
        );
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        if let Ok(appdata) = PathBuf::from(appdata).canonicalize() {
            if path.starts_with(appdata) {
                return Err("APPDATA / production database paths are refused".into());
            }
        }
    }
    if path.file_name().and_then(|s| s.to_str()) != Some("iot-dataset-validation.db") {
        return Err("only the explicit iot-dataset-validation.db fixture is accepted".into());
    }
    Ok(path)
}

async fn load_contexts(pool: &SqlitePool) -> TestResult<Vec<ProtocolContext>> {
    let protocols = db::list_protocols(pool).await?;
    require!(
        protocols.len() == 54,
        "fixture must contain exactly 54 IoT protocols; got {}",
        protocols.len()
    );
    let expected_ids: BTreeSet<_> = FAMILIES
        .iter()
        .flat_map(|(prefix, _)| {
            OPERATIONS.iter().flat_map(move |(op, _, _)| {
                [format!("{prefix}_{op}_req"), format!("{prefix}_{op}_resp")]
            })
        })
        .collect();
    let actual_ids: BTreeSet<_> = protocols.iter().map(|p| p.protocol_id.clone()).collect();
    require!(
        actual_ids == expected_ids,
        "fixture protocol IDs differ: missing {:?}, extra {:?}",
        expected_ids.difference(&actual_ids).collect::<Vec<_>>(),
        actual_ids.difference(&expected_ids).collect::<Vec<_>>()
    );
    let systems = db::list_systems(pool).await?;
    let groups = db::list_groups(pool).await?;
    require!(
        systems.len() == 3 && groups.len() == 9,
        "expected 3 systems / 9 groups, got {} / {}",
        systems.len(),
        groups.len()
    );
    let validation = e2e_validate_all_protocols(pool).await;
    require!(
        validation.is_empty(),
        "backend open/save validation failed:\n{}",
        validation.join("\n")
    );
    let header_layout = [
        ("协议标识", 2),
        ("版本", 1),
        ("业务域", 1),
        ("消息码", 1),
        ("源终端", 4),
        ("目标终端", 4),
        ("序号", 2),
        ("会话", 4),
        ("总长", 2),
    ];
    let mut result = Vec::new();
    let mut family_systems = HashSet::new();
    for (prefix, family) in FAMILIES {
        let family_protocols: Vec<_> = protocols
            .iter()
            .filter(|p| p.protocol_id.starts_with(prefix))
            .collect();
        let system_ref = family_protocols[0]
            .system_ref
            .ok_or("protocol has no system")?;
        require!(
            family_systems.insert(system_ref),
            "families unexpectedly share system {system_ref}"
        );
        require!(
            family_protocols
                .iter()
                .all(|p| p.system_ref == Some(system_ref)),
            "{prefix} is split across systems"
        );
        for (op, group_name, request_code) in OPERATIONS {
            for response in [false, true] {
                let pid = format!("{prefix}_{op}_{}", if response { "resp" } else { "req" });
                let protocol = protocols
                    .iter()
                    .find(|p| p.protocol_id == pid)
                    .unwrap()
                    .clone();
                require!(
                    protocol.format == PayloadFormat::Hex && protocol.def_mode == DefMode::Fields,
                    "{pid} must use Hex fields"
                );
                require!(
                    protocol.group_name.as_deref() == Some(group_name),
                    "{pid} has wrong group {:?}",
                    protocol.group_name
                );
                let group = groups
                    .iter()
                    .find(|g| Some(g.id) == protocol.group_ref)
                    .ok_or("protocol group missing")?;
                require!(
                    group.system_ref == system_ref && group.header_enabled,
                    "{pid} group header / ownership invalid"
                );
                let header = db::protocol_headers(&pool, &protocol).await?;
                require!(
                    header.len() == header_layout.len(),
                    "{pid} header has {} fields",
                    header.len()
                );
                for (field, (name, length)) in header.iter().zip(header_layout) {
                    require!(
                        field.name == name && field.length == length && field.enc == FieldEnc::Hex,
                        "{pid} header field differs: {:?}",
                        field
                    );
                }
                let domain = match group_name {
                    "接入管理" => "01",
                    "参数配置" => "02",
                    _ => "03",
                };
                let code = if response {
                    format!("{:02X}", u8::from_str_radix(request_code, 16)? | 0x80)
                } else {
                    request_code.into()
                };
                let fields = udp::frame_specs(&protocol, &header)?;
                for (name, value) in [
                    ("协议标识", family),
                    ("版本", "20"),
                    ("业务域", domain),
                    ("消息码", &code),
                    ("尾标", "55AA"),
                ] {
                    let f = fields
                        .iter()
                        .find(|f| f.name == name)
                        .ok_or_else(|| format!("{pid} missing {name}"))?;
                    require!(
                        compact_hex(&f.value) == value,
                        "{pid} {name} default expected {value}, got {}",
                        f.value
                    );
                }
                for (name, role) in [
                    ("源终端", FieldRole::SrcId),
                    ("目标终端", FieldRole::DstId),
                    ("会话", FieldRole::Session),
                ] {
                    require!(
                        fields.iter().any(|f| f.name == name && f.role == role),
                        "{pid} missing {name} semantic role"
                    );
                }
                require!(
                    fields.iter().all(|f| f.enc == FieldEnc::Hex),
                    "{pid} has non-Hex field"
                );
                require!(
                    fields
                        .last()
                        .is_some_and(|f| f.name == "尾标" && f.length == 2 && f.when.is_none()),
                    "{pid} must end with unconditional 2-byte 尾标"
                );
                let explicit = protocol.match_conds();
                for name in ["协议标识", "业务域", "消息码", "尾标"] {
                    require!(
                        explicit
                            .iter()
                            .any(|c| c.field == name && !c.value.is_empty() && c.value != "*"),
                        "{pid} lacks explicit identity/tail match condition {name}"
                    );
                }
                if response {
                    require!(
                        protocol.auto_respond.as_deref().unwrap_or("").is_empty(),
                        "{pid} response must not create a response loop"
                    );
                } else {
                    require!(
                        protocol.auto_respond.as_deref()
                            == Some(format!("{prefix}_{op}_resp").as_str()),
                        "{pid} automatic response pairing is wrong"
                    );
                }
                require!(protocol.auto_periodic.as_deref().unwrap_or("").is_empty(), "{pid} unexpected periodic trigger would interfere with isolated peer validation");
                let dicts = db::dict_map_for_group(pool, protocol.group_ref).await?;
                require!(
                    !dicts.is_empty(),
                    "{pid} has no resolved group dictionaries"
                );
                let conds = effective_conds(&fields, &explicit);
                result.push(ProtocolContext {
                    protocol,
                    fields,
                    dicts,
                    conds,
                });
            }
        }
    }
    for group in groups {
        require!(
            result
                .iter()
                .filter(|c| c.protocol.group_ref == Some(group.id))
                .count()
                == 6,
            "group {} does not contain exactly six protocols",
            group.name
        );
    }
    println!("PASS: 3 systems, 9 groups, 54 protocol identities, headers, dictionaries, pairings, and backend save validation");
    Ok(result)
}

fn literal_bytes(
    field: &ProtocolField,
    value: &str,
    dicts: &DictMap,
) -> TestResult<Option<Vec<u8>>> {
    if value.contains('{') {
        return Ok(None);
    }
    let digits = if let Some(key) = field.dict_key() {
        let set = dicts
            .get(key)
            .ok_or_else(|| format!("{} references missing dictionary {key}", field.name))?;
        require!(
            set.byte_len == field.length as usize,
            "{} dictionary width differs",
            field.name
        );
        if value.trim().is_empty() {
            set.default_value()
        } else {
            set.find_value(value)
        }
        .ok_or_else(|| {
            format!(
                "{} value {value:?} is absent from dictionary {key}",
                field.name
            )
        })?
        .to_string()
    } else if value.trim().is_empty() {
        "00".repeat(field.length as usize)
    } else {
        compact_hex(value)
    };
    let bytes = hex::decode(digits)?;
    require!(
        bytes.len() == field.length as usize,
        "{} literal expected {} bytes, got {}",
        field.name,
        field.length,
        bytes.len()
    );
    Ok(Some(bytes))
}

// Build the same explicit list payload a UI caller must supply. unit_fields are a
// record layout, not an engine generator; scalar defaults and dictionary values
// are assembled here, then the actual engine sends and parses the resulting frame.
fn prepare(
    ctx: &ProtocolContext,
    overrides: &Values,
    empty_lists: bool,
    request: Option<&Values>,
) -> TestResult<Prepared> {
    let mut params = overrides.clone();
    let mut values = Values::new();
    let mut expected = Vec::new();
    for (index, field) in ctx.fields.iter().enumerate() {
        if let Some(condition) = &field.when {
            let Some(actual) = values.get(&condition.field) else {
                continue;
            };
            let selector = ctx
                .fields
                .iter()
                .find(|f| f.name == condition.field)
                .ok_or("when discriminator missing")?;
            let wanted = literal_bytes(selector, &condition.value, &ctx.dicts)?
                .ok_or("when must use a literal or dictionary selector")?;
            if compact_hex(actual) != hex::encode_upper(wanted) {
                continue;
            }
        }
        if let Some(name) = req_ref_name(&field.value) {
            let value = request.and_then(|v| v.get(name)).ok_or_else(|| {
                format!("{} requires request field {name}", ctx.protocol.protocol_id)
            })?;
            params
                .entry(field.name.clone())
                .or_insert_with(|| value.clone());
        }
        if field.role == FieldRole::DeviceCount && empty_lists {
            params.insert(field.name.clone(), "00".repeat(field.length as usize));
        }
        let mut list_count = None;
        let bytes = if is_variable_len(field) {
            require!(
                field.role == FieldRole::DeviceList && field.value == "{vlen}",
                "{} unsupported variable field {}",
                ctx.protocol.protocol_id,
                field.name
            );
            let counter = ctx.fields[..index]
                .iter()
                .rev()
                .find(|f| f.role == FieldRole::DeviceCount && values.contains_key(&f.name))
                .ok_or_else(|| {
                    format!(
                        "{} list {} lacks a preceding active count",
                        ctx.protocol.protocol_id, field.name
                    )
                })?;
            let count = usize::from_str_radix(&compact_hex(&values[&counter.name]), 16)?;
            require!(
                count <= 256,
                "{} {} unexpectedly has {count} records",
                ctx.protocol.protocol_id,
                field.name
            );
            require!(
                !field.unit_fields.is_empty(),
                "{} lacks a record layout",
                field.name
            );
            let mut unit = Vec::new();
            for child in &field.unit_fields {
                require!(
                    child.enc == FieldEnc::Hex
                        && child.when.is_none()
                        && child.unit_fields.is_empty()
                        && !child.value.contains('{'),
                    "{} child {} must be a fixed Hex literal/dictionary field",
                    field.name,
                    child.name
                );
                unit.extend(
                    literal_bytes(child, &child.value, &ctx.dicts)?
                        .ok_or("unit default must be literal")?,
                );
            }
            require!(
                unit.len() == field.length as usize,
                "{} record width differs from unit_fields",
                field.name
            );
            let bytes = unit.repeat(count);
            params.insert(field.name.clone(), hex::encode_upper(&bytes));
            list_count = Some(count);
            Some(bytes)
        } else {
            let value = params
                .get(&field.name)
                .map(String::as_str)
                .unwrap_or(&field.value);
            if value == "{node_id}" {
                Some(hex::decode(TERMINAL_A)?)
            } else {
                literal_bytes(field, value, &ctx.dicts)?
            }
        };
        if let Some(bytes) = &bytes {
            values.insert(field.name.clone(), hex::encode_upper(bytes));
        } else {
            require!(
                !ctx.fields
                    .iter()
                    .any(|f| f.when.as_ref().is_some_and(|w| w.field == field.name)),
                "{} uses runtime-dependent discriminator {}; fixture cases must define its value",
                ctx.protocol.protocol_id,
                field.name
            );
        }
        expected.push(ExpectedField {
            index,
            length: bytes.as_ref().map_or(field.length as usize, Vec::len),
            bytes,
            list_count,
        });
    }
    Ok(Prepared { params, expected })
}

// One case for each when edge, recursively selecting parent branches. This
// covers nested discriminators without an exponential Cartesian product.
fn branch_cases(ctx: &ProtocolContext) -> TestResult<Vec<Values>> {
    fn select(ctx: &ProtocolContext, index: usize, values: &mut Values) -> TestResult<()> {
        if let Some(condition) = &ctx.fields[index].when {
            let parent = ctx.fields[..index]
                .iter()
                .position(|f| f.name == condition.field)
                .ok_or_else(|| {
                    format!(
                        "{} has forward/missing when discriminator {}",
                        ctx.protocol.protocol_id, condition.field
                    )
                })?;
            select(ctx, parent, values)?;
            values.insert(condition.field.clone(), condition.value.clone());
        }
        Ok(())
    }
    let mut cases = vec![Values::new()];
    let mut seen = HashSet::from([String::new()]);
    for index in 0..ctx.fields.len() {
        if ctx.fields[index].when.is_none() {
            continue;
        }
        let mut values = Values::new();
        select(ctx, index, &mut values)?;
        let ordered: BTreeMap<_, _> = values.iter().collect();
        let key = serde_json::to_string(&ordered)?;
        if seen.insert(key) {
            cases.push(values);
        }
    }
    Ok(cases)
}

fn matches<'a>(all: &'a [ProtocolContext], bytes: &[u8]) -> Vec<&'a str> {
    all.iter()
        .filter(|c| check_match_conds(&c.fields, &c.dicts, &c.conds, bytes))
        .map(|c| c.protocol.protocol_id.as_str())
        .collect()
}

fn verify_frame(
    ctx: &ProtocolContext,
    all: &[ProtocolContext],
    bytes: &[u8],
    prepared: &Prepared,
    coverage: &mut Coverage,
) -> TestResult<Values> {
    let pid = &ctx.protocol.protocol_id;
    let expected_len: usize = prepared.expected.iter().map(|e| e.length).sum();
    require!(
        bytes.len() == expected_len,
        "{pid}: expected layout length {expected_len}, received {}; {}",
        bytes.len(),
        hex::encode_upper(bytes)
    );
    let matched = matches(all, bytes);
    require!(
        matched == [pid.as_str()],
        "{pid}: expected unique match among 54, got {matched:?}"
    );
    let extracted = extract_field_values(&ctx.fields, bytes, &ctx.dicts);
    require!(
        extracted.len() == prepared.expected.len(),
        "{pid}: extraction selected {} fields; expected {}",
        extracted.len(),
        prepared.expected.len()
    );
    let mut offset = 0;
    for e in &prepared.expected {
        let field = &ctx.fields[e.index];
        let actual = &bytes[offset..offset + e.length];
        if let Some(expected) = &e.bytes {
            require!(
                actual == expected,
                "{pid} field {}: expected {}, got {}",
                field.name,
                hex::encode_upper(expected),
                hex::encode_upper(actual)
            );
        }
        let parsed = extracted
            .get(&field.name)
            .ok_or_else(|| format!("{pid}: field {} disappeared during extraction", field.name))?;
        require!(
            compact_hex(parsed) == hex::encode_upper(actual),
            "{pid}: extraction offset differs at {}",
            field.name
        );
        if field.name == "总长" {
            require!(
                actual.len() == 2
                    && u16::from_be_bytes([actual[0], actual[1]]) as usize == bytes.len(),
                "{pid}: total length header differs from UDP datagram size"
            );
        }
        if field.when.is_some() {
            coverage.branches += 1;
        }
        if let Some(count) = e.list_count {
            if count == 0 {
                coverage.empty_lists += 1;
            } else {
                coverage.populated_lists += 1;
            }
        }
        offset += e.length;
    }
    let parsed = parse_fields(&ctx.fields, bytes, &ctx.dicts);
    require!(
        parsed.iter().all(|p| p.name != "payload"
            && !p.display.contains("<缺")
            && !p.display.contains("<字典")),
        "{pid}: truncated, leftover, or undefined dictionary display: {parsed:?}"
    );
    require!(
        extracted
            .get("尾标")
            .is_some_and(|v| compact_hex(v) == "55AA")
            && bytes.ends_with(&[0x55, 0xAA]),
        "{pid}: tail was lost or absorbed by a list"
    );
    coverage.frames += 1;
    Ok(extracted)
}

fn verify_negative_matches(
    ctx: &ProtocolContext,
    all: &[ProtocolContext],
    bytes: &[u8],
    coverage: &mut Coverage,
) -> TestResult<()> {
    for index in [0, 3, 4, bytes.len() - 1] {
        let mut corrupt = bytes.to_vec();
        corrupt[index] ^= 0x40;
        require!(
            matches(all, &corrupt).is_empty(),
            "{}: corrupted family/domain/message/tail at byte {index} still matched {:?}",
            ctx.protocol.protocol_id,
            matches(all, &corrupt)
        );
        coverage.negative_matches += 1;
    }
    require!(
        matches(all, &bytes[..bytes.len() - 1]).is_empty(),
        "{}: truncated tail still matched",
        ctx.protocol.protocol_id
    );
    coverage.negative_matches += 1;
    Ok(())
}

async fn receive(peer: &UdpSocket) -> TestResult<(Vec<u8>, SocketAddr)> {
    let mut buffer = vec![0; 65535];
    let (n, source) = tokio::time::timeout(Duration::from_secs(5), peer.recv_from(&mut buffer))
        .await
        .map_err(|_| "loopback peer timed out waiting for actual engine datagram")??;
    buffer.truncate(n);
    require!(
        source.ip().is_loopback(),
        "received non-loopback UDP source {source}"
    );
    Ok((buffer, source))
}

async fn send<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    engine: &Arc<EngineState>,
    node: &Node,
    peer: &UdpSocket,
    ctx: &ProtocolContext,
    prepared: &Prepared,
) -> TestResult<Vec<u8>> {
    let sent = udp::send_datagram(
        app,
        pool,
        engine,
        node,
        &ctx.protocol,
        &prepared.params,
        None,
    )
    .await?;
    let (bytes, source) = receive(peer).await?;
    require!(
        source.port() == node.local_port as u16,
        "{} came from an unexpected socket",
        ctx.protocol.protocol_id
    );
    require!(
        sent.bytes == bytes.len() && compact_hex(&sent.hex) == hex::encode_upper(&bytes),
        "{} SendResult differs from actual UDP bytes",
        ctx.protocol.protocol_id
    );
    Ok(bytes)
}

async fn log_base(pool: &SqlitePool, node: &Node) -> TestResult<i64> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(id) FROM logs WHERE node_ref=?")
            .bind(node.id)
            .fetch_one(pool)
            .await?
            .unwrap_or(0),
    )
}

async fn wait_log(
    pool: &SqlitePool,
    node: &Node,
    after: i64,
    direction: &str,
    pid: &str,
    bytes: &[u8],
) -> TestResult<()> {
    let wire = hex::encode_upper(bytes);
    for _ in 0..200 {
        let rows: Vec<(Option<String>, Option<String>, String)> = sqlx::query_as("SELECT protocol_id, raw_hex, status FROM logs WHERE node_ref=? AND id>? AND direction=? ORDER BY id")
            .bind(node.id).bind(after).bind(direction).fetch_all(pool).await?;
        for (actual_pid, raw, status) in &rows {
            require!(
                status != "error" && status != "no-match",
                "{pid}: engine {direction} logged {status} after log {after}; rows={rows:?}"
            );
            if actual_pid.as_deref() == Some(pid)
                && raw.as_deref().is_some_and(|v| compact_hex(v) == wire)
            {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let diagnostics: Vec<(String, Option<String>, String, Option<String>)> = sqlx::query_as("SELECT direction,protocol_id,status,parsed FROM logs WHERE node_ref=? AND id>? ORDER BY id")
        .bind(node.id).bind(after).fetch_all(pool).await?;
    Err(format!("{pid}: missing {direction} log for actual datagram; {diagnostics:?}").into())
}

fn context<'a>(all: &'a [ProtocolContext], pid: &str) -> TestResult<&'a ProtocolContext> {
    all.iter()
        .find(|c| c.protocol.protocol_id == pid)
        .ok_or_else(|| format!("missing context {pid}").into())
}

async fn automatic_pair<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    engine: &Arc<EngineState>,
    node: &Node,
    peer: &UdpSocket,
    address: SocketAddr,
    all: &[ProtocolContext],
    request: &ProtocolContext,
    identity: &str,
    coverage: &mut Coverage,
) -> TestResult<Values> {
    let params = Values::from([("源终端".into(), identity.into())]);
    let prepared = prepare(request, &params, false, None)?;
    let bytes = send(app, pool, engine, node, peer, request, &prepared).await?;
    let request_values = verify_frame(request, all, &bytes, &prepared, coverage)?;
    let response = context(
        all,
        request
            .protocol
            .auto_respond
            .as_deref()
            .ok_or("missing auto response")?,
    )?;
    let response_prepared = prepare(response, &Values::new(), false, Some(&request_values))?;
    let base = log_base(pool, node).await?;
    peer.send_to(&bytes, address).await?;
    // This receive is the real automatic reply. No response render/send is called here.
    let (reply, source) = receive(peer)
        .await
        .map_err(|e| format!("{} automatic reply: {e}", request.protocol.protocol_id))?;
    require!(
        source == address,
        "automatic response came from {source}, expected {address}"
    );
    let response_values = verify_frame(response, all, &reply, &response_prepared, coverage)?;
    require!(
        compact_hex(&response_values["目标终端"]) == identity
            && compact_hex(&response_values["源终端"]) == "00000000",
        "{} automatic reply addresses were not reversed correctly",
        response.protocol.protocol_id
    );
    require!(
        response_values["序号"] == request_values["序号"],
        "{} did not echo request sequence",
        response.protocol.protocol_id
    );
    require!(
        response_values
            .get("确认消息码")
            .is_some_and(|value| compact_hex(value) == compact_hex(&request_values["消息码"])),
        "{} 确认消息码 must echo actual request 消息码 {}; got {:?}",
        response.protocol.protocol_id,
        request_values["消息码"],
        response_values.get("确认消息码")
    );
    wait_log(
        pool,
        node,
        base,
        "in",
        &request.protocol.protocol_id,
        &bytes,
    )
    .await?;
    wait_log(
        pool,
        node,
        base,
        "out",
        &response.protocol.protocol_id,
        &reply,
    )
    .await?;
    // Feed the observed server reply to the real receiving engine to validate
    // response recognition, capture semantics, and terminal profile persistence.
    let base = log_base(pool, node).await?;
    peer.send_to(&reply, address).await?;
    wait_log(
        pool,
        node,
        base,
        "in",
        &response.protocol.protocol_id,
        &reply,
    )
    .await?;
    coverage.automatic_pairs += 1;
    Ok(response_values)
}

async fn exercise<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    engine: &Arc<EngineState>,
    node: &Node,
    peer: &UdpSocket,
    all: &[ProtocolContext],
) -> TestResult<Coverage> {
    let socket = udp::ensure_server(app, pool, engine, node).await?;
    let address = socket.local_addr()?;
    require!(
        address.ip().is_loopback() && address.port() == node.local_port as u16,
        "engine did not bind isolated loopback address"
    );
    let mut coverage = Coverage::default();
    let mut baseline_requests: HashMap<String, Values> = HashMap::new();
    for ctx in all {
        let request_pid = ctx
            .protocol
            .protocol_id
            .strip_suffix("_resp")
            .map(|s| format!("{s}_req"));
        let request_values = request_pid
            .as_ref()
            .and_then(|pid| baseline_requests.get(pid))
            .cloned();
        let cases = branch_cases(ctx)?;
        let mut visited = HashSet::new();
        let mut zero_lists = HashSet::new();
        let mut populated_lists = HashSet::new();
        let mut count = 0;
        for (case_number, overrides) in cases.iter().enumerate() {
            let has_lists = ctx.fields.iter().any(is_variable_len);
            for empty in [false, true] {
                if empty && !has_lists {
                    continue;
                }
                let prepared = prepare(ctx, overrides, empty, request_values.as_ref())?;
                let bytes = send(app, pool, engine, node, peer, ctx, &prepared)
                    .await
                    .map_err(|e| {
                        format!(
                            "{} branch case {case_number}, zero={empty}: {e}",
                            ctx.protocol.protocol_id
                        )
                    })?;
                let values = verify_frame(ctx, all, &bytes, &prepared, &mut coverage)?;
                if case_number == 0 && !empty {
                    verify_negative_matches(ctx, all, &bytes, &mut coverage)?;
                    if request_pid.is_none() {
                        baseline_requests.insert(ctx.protocol.protocol_id.clone(), values);
                    }
                }
                for expected in &prepared.expected {
                    visited.insert(expected.index);
                    match expected.list_count {
                        Some(0) => {
                            zero_lists.insert(expected.index);
                        }
                        Some(_) => {
                            populated_lists.insert(expected.index);
                        }
                        None => {}
                    }
                }
                count += 1;
            }
        }
        for (index, field) in ctx.fields.iter().enumerate() {
            require!(
                field.when.is_none() || visited.contains(&index),
                "{} conditional field {} was never exercised",
                ctx.protocol.protocol_id,
                field.name
            );
            if is_variable_len(field) {
                require!(
                    zero_lists.contains(&index) && populated_lists.contains(&index),
                    "{} list {} lacks both populated and zero-count coverage",
                    ctx.protocol.protocol_id,
                    field.name
                );
            }
        }
        println!(
            "PASS manual: {} ({count} branch/list scenarios)",
            ctx.protocol.protocol_id
        );
    }
    for request in all
        .iter()
        .filter(|c| c.protocol.protocol_id.ends_with("_req"))
    {
        automatic_pair(
            app,
            pool,
            engine,
            node,
            peer,
            address,
            all,
            request,
            TERMINAL_A,
            &mut coverage,
        )
        .await?;
    }
    require!(
        coverage.automatic_pairs == 27,
        "expected 27 automatic request/response pairs"
    );
    println!(
        "PASS: all 27 actual automatic request/response pairs and incoming response recognition"
    );

    let mut sessions = HashMap::new();
    let mut distinct = HashSet::new();
    for (prefix, _) in FAMILIES {
        let registration = context(all, &format!("{prefix}_register_req"))?;
        for identity in [TERMINAL_A, TERMINAL_B] {
            let values = automatic_pair(
                app,
                pool,
                engine,
                node,
                peer,
                address,
                all,
                registration,
                identity,
                &mut coverage,
            )
            .await?;
            let session = compact_hex(&values["会话"]);
            require!(
                session.len() == 8 && session != "00000000",
                "{prefix}/{identity} received invalid random registration session {session}"
            );
            require!(
                distinct.insert(session.clone()),
                "two registrations returned the same random session {session}"
            );
            let system = registration
                .protocol
                .system_ref
                .ok_or("registration has no system")?;
            let fresh = db::get_node(pool, node.id).await?;
            let bucket = format!("s{system}:{identity}");
            require!(
                fresh
                    .capture_buckets()
                    .get(&bucket)
                    .and_then(|b| b.get("__session"))
                    == Some(&session),
                "{prefix}/{identity}: capture bucket {bucket} did not persist {session}"
            );
            let stored: Option<String> = sqlx::query_scalar("SELECT session_hex FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?")
                .bind(node.id).bind(system).bind(identity).fetch_one(pool).await?;
            require!(
                stored.as_deref() == Some(session.as_str()),
                "{prefix}/{identity}: terminal profile session differs"
            );
            sessions.insert((prefix, identity), session);
        }
    }
    // All sends deliberately retain the original stale Node snapshot. The engine
    // must fetch the current bucket and select it by system + outgoing identity.
    for identity in [TERMINAL_A, TERMINAL_B, TERMINAL_C, TERMINAL_A] {
        for (prefix, _) in FAMILIES {
            let heartbeat = context(all, &format!("{prefix}_heartbeat_req"))?;
            let params = Values::from([("源终端".into(), identity.into())]);
            let prepared = prepare(heartbeat, &params, false, None)?;
            let bytes = send(app, pool, engine, node, peer, heartbeat, &prepared).await?;
            let values = verify_frame(heartbeat, all, &bytes, &prepared, &mut coverage)?;
            let expected = sessions
                .get(&(prefix, identity))
                .map(String::as_str)
                .unwrap_or("00000000");
            require!(
                compact_hex(&values["会话"]) == expected,
                "{prefix}/{identity}: heartbeat session should be {expected}, got {}",
                values["会话"]
            );
        }
    }
    require!(
        coverage.branches > 0 && coverage.populated_lists > 0 && coverage.empty_lists > 0,
        "fixture did not exercise branches and populated/empty lists"
    );
    println!("PASS: A/B registrations in all 3 families, persisted scoped captures/profiles, unregistered C, switching back, and stale-node session refresh");
    Ok(coverage)
}

#[tokio::main]
async fn main() -> TestResult<()> {
    let path = fixture_path()?;
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await?;
    let all = load_contexts(&pool).await?;
    let peer = UdpSocket::bind("127.0.0.1:0").await?;
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let port = reservation.local_addr()?.port();
    drop(reservation);
    let groups: BTreeSet<_> = all.iter().filter_map(|c| c.protocol.group_ref).collect();
    let node_id: i64 = sqlx::query_scalar("INSERT INTO nodes(name,role,local_ip,local_port,remote_ip,remote_port,node_id_hex,group_bind_json) VALUES('IoT fixture E2E','simulator','127.0.0.1',?,'127.0.0.1',?, ?, ?) RETURNING id")
        .bind(i64::from(port)).bind(i64::from(peer.local_addr()?.port())).bind(TERMINAL_A).bind(serde_json::to_string(&groups)?).fetch_one(&pool).await?;
    let node = db::get_node(&pool, node_id).await?;
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let engine = Arc::new(EngineState::new());
    let result = exercise(&handle, &pool, &engine, &node, &peer, &all).await;
    udp::stop_server(&handle, &engine, node_id);
    require!(
        engine.sockets.is_empty() && engine.listeners.is_empty() && engine.periodic.is_empty(),
        "engine cleanup did not release test tasks/sockets"
    );
    // Remove only the harness-created channel and its FK-owned terminal profiles.
    // Logs remain in the explicit fixture as evidence (node_ref becomes NULL).
    sqlx::query("DELETE FROM nodes WHERE id=? AND name='IoT fixture E2E'")
        .bind(node_id)
        .execute(&pool)
        .await?;
    pool.close().await;
    let coverage = result?;
    println!("PASS IoT fixture: {} UDP frames, {} branch fields, {} populated lists, {} empty lists, {} automatic pairs, {} corrupt/truncated non-matches; fixture={}", coverage.frames, coverage.branches, coverage.populated_lists, coverage.empty_lists, coverage.automatic_pairs, coverage.negative_matches, path.display());
    Ok(())
}
