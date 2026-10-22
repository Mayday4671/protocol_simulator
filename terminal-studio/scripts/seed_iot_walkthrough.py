"""Add an isolated, explicitly custom IoT walkthrough. Never replace existing rows.
Usage: python scripts/seed_iot_walkthrough.py --db ABSOLUTE_DB --backup ABSOLUTE_BACKUP
The application must already have initialized/migrated the target database.
"""
import argparse
import copy
import json
import sqlite3
from pathlib import Path

PREFIX = 'iot_walk_v1'
SYSTEM = '物联网联调协议 v1（自定义）'
OPS = [('register', '注册设备', 0x01), ('heartbeat', '心跳保活', 0x02),
       ('configure', '配置采样参数', 0x10), ('query', '查询设备状态', 0x11),
       ('control', '控制继电器', 0x20), ('logout', '注销设备', 0x7F)]


def field(name, length, value='', role='none', **extra):
    return dict(name=name, len=length, enc='hex', value=value, role=role, **extra)


def insert_exchanges(con, sid, gid, steps):
    ids = []
    for source in steps:
        step = copy.deepcopy(source)
        step.update(x=240, y=60, on_success=None, on_failure=None,
                    on_timeout=None, next=None, delay_ms=0)
        ids.append(con.execute(
            '''INSERT INTO business_flows(name,system_ref,group_ref,description,
               steps_json,trigger_protocols_json,schedule_interval_ms)
               VALUES(?,?,?,?,?,'[]',0)''',
            (step['name'] + ' · 请求与响应', sid, gid,
             '独立交互；应答流水号及反向设备地址必须匹配，处理结果 00 为成功。',
             json.dumps([step], ensure_ascii=False))).lastrowid)
    return ids


def install(db_path, backup, local_port=39161, remote_port=39162):
    db_path, backup = Path(db_path).resolve(), Path(backup).resolve()
    if not db_path.is_file() or backup.exists() or db_path == backup:
        raise ValueError('Target must exist; backup must be a new, different path')
    backup.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(db_path, timeout=10)
    con.execute('PRAGMA foreign_keys=ON')
    with sqlite3.connect(backup) as dest:
        con.backup(dest)
    j = lambda x: json.dumps(x, ensure_ascii=False)
    try:
        con.execute('BEGIN IMMEDIATE')
        if con.execute('SELECT 1 FROM protocol_systems WHERE name=?', (SYSTEM,)).fetchone():
            raise ValueError('Walkthrough already exists; existing data was not changed')
        sid = con.execute('INSERT INTO protocol_systems(name,description) VALUES(?,?)',
                          (SYSTEM, '自定义二进制 UDP 联调协议，不是行业标准。注册→心跳→参数配置→状态查询→控制→注销；配套本机模拟平台。')).lastrowid
        header = [field('同步字', 2, 'C0DE'), field('版本', 1, '01'),
                  field('消息码', 1, '01', 'type_code'), field('源设备', 2, '0001', 'src_id'),
                  field('目标设备', 2, '0000', 'dst_id'), field('流水号', 2, '{seq}', 'correlation'),
                  field('会话令牌', 2, '{session}'), field('报文总长', 2, '{tlen}')]
        hid = PREFIX + '_header'
        gid = con.execute('INSERT INTO protocol_groups(system_ref,name,header_enabled,header_profiles_json) VALUES(?,?,?,?)',
                          (sid, '设备交互', 0, j([dict(id=hid, name='物联网公共头', fields=header)]))).lastrowid
        dictionaries = {
            'result': ('处理结果', [('00', '成功'), ('01', '不支持的命令'), ('02', '参数无效'), ('03', '未注册'), ('04', '会话无效')]),
            'relay': ('继电器状态', [('00', '关闭'), ('01', '开启')]),
            'state': ('设备状态', [('00', '正常'), ('01', '告警')]),
            'model': ('设备型号', [('01', '环境采集终端')]),
        }
        for key, (name, entries) in dictionaries.items():
            did = con.execute('INSERT INTO dict_sets(key,name,system_ref,group_ref,byte_len) VALUES(?,?,?,?,1)',
                              (PREFIX+'_'+key, name, sid, gid)).lastrowid
            con.executemany('INSERT INTO dict_items(set_ref,value,label,sort_order) VALUES(?,?,?,?)',
                            [(did, value, label, i) for i, (value, label) in enumerate(entries)])
        result = field('处理结果', 1, '00', 'result', dict=PREFIX+'_result')
        reqs = {
            'register': [field('设备型号', 1, '01', dict=PREFIX+'_model'), field('固件版本', 2, '0100'), field('设备序列号', 4, '01020304', profile_key='serial')],
            'heartbeat': [field('运行秒数', 4, '0000003C'), field('设备状态', 1, '00', dict=PREFIX+'_state')],
            'configure': [field('采样周期（秒）', 2, '000A'), field('告警温度（0.1℃）', 2, '012C')],
            'query': [field('查询范围', 1, '01')],
            'control': [field('继电器状态', 1, '01', dict=PREFIX+'_relay')],
            'logout': [field('注销原因', 1, '00')],
        }
        replies = {
            'register': [result, field('建议心跳周期（秒）', 2, '001E')],
            'heartbeat': [result, field('平台时间戳', 4, '{timestamp}')],
            'configure': [result, field('生效采样周期（秒）', 2, '000A')],
            'query': [result, field('温度（0.1℃）', 2, '00FA'), field('湿度（0.1%）', 2, '01F4'), field('继电器状态', 1, '00', dict=PREFIX+'_relay'), field('当前采样周期（秒）', 2, '000A')],
            'control': [result, field('继电器状态', 1, '01', dict=PREFIX+'_relay')],
            'logout': [result],
        }
        for op, name, code in OPS:
            for response in [False, True]:
                wire_code = f'{code | (0x80 if response else 0):02X}'
                override = field('消息码', 1, wire_code, 'type_code', hdr_ref=hid+'::消息码')
                overrides = [override]
                # Only registration assigns a session. Other replies merely echo it.
                if not response or op == 'register':
                    overrides.append(field('会话令牌', 2, '{session}', 'session', hdr_ref=hid+'::会话令牌'))
                body = replies[op] if response else reqs[op]
                con.execute('''INSERT INTO protocols(protocol_id,name,format,msg_type,payload_template,def_mode,fields_json,
                               system_ref,group_ref,match_fields_json,header_refs_json,description) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)''',
                            (PREFIX+'_'+op+('_resp' if response else '_req'), f'0x{wire_code} {name}'+('响应' if response else ''),
                             'hex', 'Response' if response else 'Request', '', 'fields', j(overrides+body), sid, gid,
                             j([dict(field='同步字',value='C0DE'),dict(field='版本',value='01'),dict(field='消息码',value=wire_code)]),j([hid]),
                             '联调协议 v1：大端；应答回显流水号，交换源/目标设备；00 表示业务成功。'))
        nid = con.execute('INSERT INTO nodes(name,role,local_ip,local_port,remote_ip,remote_port,enabled,group_bind_json) VALUES(?,?,?,?,?,?,?,?)',
                          ('物联网本机联调', 'simulator','127.0.0.1',local_port,'127.0.0.1',remote_port,1,j([gid]))).lastrowid
        tid = con.execute('INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES(?,?,?,?,?,?)',
                          ('环境采集终端 01',nid,sid,PREFIX+'_register_req','0001',j(dict(serial='01020304')))).lastrowid
        steps=[]
        for i,(op,name,code) in enumerate(OPS):
            steps.append(dict(id=op,name=name,x=240,y=60+i*180,protocol_id=PREFIX+'_'+op+'_req',
                expect_protocol=PREFIX+'_'+op+'_resp',terminal_mode='single',terminal_id=None,params={},action='send',
                timeout_ms=60000,delay_ms=600,correlation=[],allow_unkeyed_reply=False,result_field='处理结果',success_hex='00',
                on_success=OPS[i+1][0] if i+1<len(OPS) else None,on_failure=None,on_timeout=None,next=None))
        flow_ids = insert_exchanges(con, sid, gid, steps)
        con.commit()
        return dict(system_id=sid,group_id=gid,node_id=nid,terminal_id=tid,flow_ids=flow_ids,protocols=12,backup=str(backup))
    except Exception:
        con.rollback()
        raise
    finally:
        con.close()


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--db',required=True);p.add_argument('--backup',required=True)
    p.add_argument('--local-port',type=int,default=39161);p.add_argument('--remote-port',type=int,default=39162)
    a=p.parse_args();print(json.dumps(install(a.db,a.backup,a.local_port,a.remote_port),ensure_ascii=False))
