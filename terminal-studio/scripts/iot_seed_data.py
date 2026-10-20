"""Deterministic synthetic IoT protocols in the application's native XML format.

These are laboratory protocols, not implementations of a published standard.
Only structures supported by the current encoder are emitted.
"""
import xml.etree.ElementTree as ET

FAMILIES = (
    ('env', '环境采集协议', 'E101', ('测点', '环境阈值', '联动策略'),
     ('温度探头', '湿度探头', '气压探头', '二氧化碳探头', '光照探头', '噪声计', '水浸探头', '颗粒物探头'),
     ('采集周期', '温度上限', '温度下限', '湿度上限', '湿度下限', '校准偏移', '上报间隔', '消抖时间', '告警延迟', '量程', '滤波窗口', '离线阈值'),
     ('超温', '低温', '湿度越限', '水浸', '空气质量', '传感器断线', '校准失效', '电量不足')),
    ('access', '门禁安防协议', 'D201', ('门锁', '凭证', '通行规则'),
     ('门锁控制器', '摆闸控制器', '读卡器', '门磁', '出门按钮', '人脸终端', '访客机', '梯控模块'),
     ('开门时长', '门磁延迟', '防拆阈值', '认证超时', '通行间隔', '访客有效期', '互锁时间', '反潜回窗口', '离线额度', '失败次数', '蜂鸣时长', '补传周期'),
     ('门未关闭', '非法开门', '防拆', '认证失败', '重复刷卡', '通行超时', '互锁冲突', '黑名单命中')),
    ('gateway', '边缘网关协议', 'F301', ('下挂设备', '路由', '转发策略'),
     ('采集网关', '协议转换器', '工业路由器', '边缘计算盒', '串口集中器', '无线协调器', '边缘存储器', '冗余控制器'),
     ('轮询周期', '转发超时', '重试次数', '缓存容量', '批量阈值', '路由权重', '链路保活', '队列深度', '限流速率', '离线时长', '同步窗口', '重连间隔'),
     ('链路断开', '路由不可达', '缓存溢出', '协议不匹配', '转发超时', '设备离线', '磁盘不足', '主备切换')),
)
GROUPS = (
    ('接入管理', '01', (('register', '终端注册', 0x01), ('unregister', '终端注销', 0x02), ('heartbeat', '心跳保活', 0x03))),
    ('参数配置', '02', (('read', '读取参数', 0x11), ('write', '批量配置', 0x12), ('topology', '查询拓扑', 0x13))),
    ('业务上报', '03', (('realtime', '实时上报', 0x21), ('alarm', '告警上报', 0x22), ('batch', '历史补传', 0x23))),
)


def field(parent, name, size, value, dictionary=None, role=None, when=None, header=False, tag='field'):
    attrs = {'name': name, 'len': str(size), 'enc': 'hex', 'value': value}
    if dictionary:
        attrs['dict'] = dictionary
    if role:
        attrs['role'] = role
    if when:
        attrs.update({'when-field': when[0], 'when-value': when[1]})
    if header:
        attrs['header-ref'] = name
    return ET.SubElement(parent, tag, attrs)


def dictionary(parent, key, name, size, labels, start=0):
    element = ET.SubElement(parent, 'dict', {'key': key, 'name': name, 'byte-len': str(size)})
    entries = labels if isinstance(labels, dict) else {f'{i:0{size * 2}X}': label for i, label in enumerate(labels, start)}
    for value, label in entries.items():
        ET.SubElement(element, 'item', {'value': value, 'label': label})


def group_dictionaries(group, family, group_name, operations):
    _, _, _, branches, devices, parameters, alarms = family
    messages = {f'{code + offset:02X}': label + suffix for _, label, code in operations
                for offset, suffix in ((0, '请求'), (0x80, '应答'))}
    dictionary(group, 'message_code', '消息类型', 1, messages)
    dictionary(group, 'mode', '业务模式', 1, branches, 1)
    dictionary(group, 'result', '处理结果', 1, ('成功', '失败', '忙，请稍后重试', '权限不足'))
    dictionary(group, 'device_type', '设备类型', 1,
               [f'{device}·{model}' for model in ('标准型', '低功耗型', '工业型') for device in devices], 1)
    dictionary(group, 'channel_id', '逻辑通道', 1, [f'{branches[0]}通道 {i:02}' for i in range(1, 33)], 1)
    dictionary(group, 'zone_id', '部署区域', 2,
               [f'{building}号楼·{floor}层·{area}区' for building in ('A', 'B') for floor in (1, 2, 3) for area in ('东', '南', '西', '北')], 1)
    dictionary(group, 'parameter_key', '参数编码', 2,
               [f'{name}·配置集 {bank}' for bank in range(1, 5) for name in parameters], 1)
    dictionary(group, 'alarm_code', '告警编码', 2,
               [f'{name}·检测通道 {channel}' for channel in range(1, 9) for name in alarms], 0x101)
    dictionary(group, 'error_code', '错误编码', 2,
               [f'{part}：{reason}' for part in ('接入', '认证', '参数', '采集', '缓存', '路由', '转发', '存储', '时钟', '固件', '权限', '同步')
                for reason in ('未就绪', '超时', '长度错误', '编码错误', '校验失败', '资源不足', '状态冲突', '版本不兼容')], 0x101)
    dictionary(group, 'vendor_id', '实验设备系列', 2,
               [f'{name}实验系列·{model}' for name in ('青禾', '云杉', '星河', '远川', '微岚', '栖木', '晨露', '岩石')
                for model in ('A', 'B', 'C')], 0x1001)
    dictionary(group, 'priority', '调度优先级', 1, ('后台', '低', '普通', '中', '高', '紧急', '关键', '保留通道'))
    dictionary(group, 'operation', '操作类型', 1, ('查询', '设置', '校验', '恢复默认'), 1)
    dictionary(group, 'sample_quality', '数据质量', 1,
               ('有效', '估算', '超量程', '欠量程', '过期', '缺失', '替代值', '待校准'))
    dictionary(group, 'unit_code', '工程单位', 1,
               ('无量纲', '摄氏度', '相对湿度', '帕斯卡', 'ppm', '勒克斯', '分贝', '微克每立方米', '毫秒', '秒', '分钟', '次', '字节', '千字节', '伏特', '毫安'))


def header(group, marker, domain, first_code):
    h = ET.SubElement(group, 'header')
    for name, size, value, dictionary_key, role in (
        ('协议标识', 2, marker, None, None), ('版本', 1, '20', None, None),
        ('业务域', 1, domain, None, None), ('消息码', 1, f'{first_code:02X}', 'message_code', 'type_code'),
        ('源终端', 4, '{node_id}', None, 'src_id'), ('目标终端', 4, '00000000', None, 'dst_id'),
        ('序号', 2, '{seq}', None, 'correlation'), ('会话', 4, '{session}', None, 'session'),
        ('总长', 2, '{tlen}', None, None),
    ):
        field(h, name, size, value, dictionary_key, role)


def extension_list(p):
    field(p, '扩展条目数', 1, '02', role='device_count')
    units = field(p, '扩展列表', 6, '{vlen}', role='device_list')
    field(units, '参数编码', 2, '0001', 'parameter_key', tag='unit')
    field(units, '参数值', 4, '00000064', tag='unit')


def complex_body(p, family, operation):
    branches = family[3]
    mode = {'read': '01', 'write': '01', 'topology': '03', 'realtime': '01', 'alarm': '02', 'batch': '03'}[operation]
    field(p, '操作', 1, '02' if operation == 'write' else '01', 'operation')
    field(p, '业务模式', 1, mode, 'mode')
    field(p, '调度优先级', 1, '05' if operation == 'alarm' else '02', 'priority')
    field(p, '工程单位', 1, '01' if family[0] == 'env' else '00', 'unit_code')
    if operation == 'alarm':
        field(p, '告警编码', 2, '0101', 'alarm_code')
        field(p, '错误编码', 2, '0101', 'error_code')
    if operation == 'batch':
        field(p, '批次编号', 4, '00000101')
        field(p, '起始时间', 4, '69B00000')
        field(p, '结束时间', 4, '69B00E10')
    for index, prefix in enumerate(branches, 1):
        when = ('业务模式', f'{index:02X}')
        field(p, prefix + '数量', 1, ('03', '04', '08')[index - 1], role='device_count', when=when)
        unit = field(p, prefix + '列表', 16, '{vlen}', role='device_list', when=when)
        value_name, value = {'env': ('采集值', '000000FA'), 'access': ('凭证标识', 'AC000001'), 'gateway': ('下一跳地址', '0A000001')}[family[0]]
        for name, size, default, key in (
            ('设备编号', 4, '01000001', None), ('通道', 1, '01', 'channel_id'),
            ('设备类型', 1, f'{index:02X}', 'device_type'), (value_name, 4, value, None),
            ('区域', 2, '0001', 'zone_id'), ('质量', 1, '00', 'sample_quality'),
            ('启用', 1, '01', 'enabled'), ('设备系列', 2, '1001', 'vendor_id'),
        ):
            field(unit, name, size, default, key, tag='unit')
        field(p, prefix + '版本', 2, '0201', when=when)
        field(p, prefix + '选项', 1, '01', 'enabled', when=when)
        field(p, prefix + '扩展模式', 1, '01', 'transport', when=when)
        for name, size, value, nested in (
            ('高级阈值', 2, '03E8', '01'), ('时间窗', 2, '003C', '01'),
            ('备用地址', 4, '0A000002', '02'), ('重试次数', 1, '03', '02'),
            ('离线容量', 4, '00001000', '03'), ('同步周期', 2, '012C', '03'),
        ):
            field(p, prefix + name, size, value, when=(prefix + '扩展模式', nested))
    extension_list(p)


def access_body(p, operation):
    field(p, '设备类型', 1, '01', 'device_type')
    field(p, '设备系列', 2, '1001', 'vendor_id')
    field(p, '部署区域', 2, '0001', 'zone_id')
    if operation == 'register':
        field(p, '硬件版本', 2, '0102')
        field(p, '软件版本', 4, '02010007')
        field(p, '固件通道', 1, '01', 'firmware_channel')
        field(p, '安全等级', 1, '02', 'security_level')
        field(p, '请求租期', 2, '0E10')
    elif operation == 'unregister':
        field(p, '注销原因', 1, '00')
        field(p, '累计运行秒', 4, '00015180')
    else:
        field(p, '累计运行秒', 4, '00015180')
        field(p, '链路质量', 1, '64')
        field(p, '电池电压', 2, '0CE4')
        field(p, '待传数量', 2, '000C')
    extension_list(p)


def response_body(p):
    field(p, '结果', 1, '00', 'result', role='result' if '_write_' in p.get('id', '') else None)
    field(p, '确认消息码', 1, '{req:消息码}', 'message_code')
    field(p, '服务器时间', 4, '{timestamp}')
    field(p, '授权租期', 2, '0E10', when=('结果', '00'))
    field(p, '同步标记', 1, '01', 'enabled', when=('结果', '00'))
    field(p, '错误编码', 2, '0101', 'error_code', when=('结果', '01'))
    field(p, '失败阶段', 1, '01', when=('结果', '01'))
    field(p, '重试间隔', 2, '001E', when=('结果', '02'))
    field(p, '要求安全等级', 1, '03', 'security_level', when=('结果', '03'))


def build_tree():
    root = ET.Element('simterm-import')
    root.append(ET.Comment('Synthetic IoT lab v2.0: three isolated families; all editable values are hexadecimal.'))
    dictionary(root, 'enabled', '启用状态', 1, ('停用', '启用'))
    dictionary(root, 'transport', '扩展通道', 1, ('实时通道', '备用通道', '离线通道'), 1)
    dictionary(root, 'firmware_channel', '固件发布通道', 1,
               ('稳定版', '长期支持版', '预发布版', '验证版', '工厂版', '维护版', '应急版', '归档版'), 1)
    dictionary(root, 'security_level', '安全等级', 1, ('实验免认证', '设备认证', '设备和会话认证', '双向认证'))
    for family in FAMILIES:
        key, name, marker = family[:3]
        system = ET.SubElement(root, 'system', {'name': name, 'description': 'IoT Lab 2.0 合成联调协议'})
        for group_name, domain, operations in GROUPS:
            group = ET.SubElement(system, 'group', {'name': group_name})
            group_dictionaries(group, family, group_name, operations)
            header(group, marker, domain, operations[0][2])
            for operation, label, request_code in operations:
                for response in (False, True):
                    pid = f'iot2_{key}_{operation}_' + ('resp' if response else 'req')
                    code = f'{request_code + (0x80 if response else 0):02X}'
                    attrs = {'id': pid, 'name': label + ('应答' if response else '请求'), 'type': 'Response' if response else 'Request'}
                    if not response:
                        attrs['auto-respond'] = f'iot2_{key}_{operation}_resp'
                    p = ET.SubElement(group, 'protocol', attrs)
                    field(p, '消息码', 1, code, 'message_code', 'type_code', header=True)
                    if response:
                        field(p, '源终端', 4, '00000000', role='src_id', header=True)
                        field(p, '目标终端', 4, '{req:源终端}', role='dst_id', header=True)
                        field(p, '序号', 2, '{req:序号}', header=True)
                        field(p, '会话', 4, '{random}' if operation == 'register' else '{req:会话}', role='session', header=True)
                        response_body(p)
                    elif domain == '01':
                        access_body(p, operation)
                    else:
                        # These stable properties are shared with registration and later commands.
                        field(p, '设备类型', 1, '01', 'device_type')
                        field(p, '设备系列', 2, '1001', 'vendor_id')
                        field(p, '部署区域', 2, '0001', 'zone_id')
                        complex_body(p, family, operation)
                    if not response:
                        common = {'设备类型': 'device_type', '设备系列': 'vendor', '部署区域': 'zone'}
                        for f in p.findall('field'):
                            if f.get('name') in common:
                                f.set('profile-key', common[f.get('name')])
                        if operation == 'write':
                            # The entire active config branch is the device's editable configuration.
                            for index, f in enumerate(p.findall('field')):
                                if not f.get('header-ref') and not f.get('profile-key'):
                                    f.set('profile-key', f'config_{index:02}')
                    field(p, '尾标', 2, '55AA')
                    match = ET.SubElement(p, 'match')
                    for name, value in (('协议标识', marker), ('业务域', domain), ('消息码', code), ('尾标', '55AA')):
                        ET.SubElement(match, 'cond', {'field': name, 'value': value})
    return ET.ElementTree(root)
