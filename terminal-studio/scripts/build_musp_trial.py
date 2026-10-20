"""Convert archived protocol *data* to the original base's simterm-import format.
No archived engine is imported. Config messages use dynamic arrays and checksums;
remaining messages retain their explicitly labelled static CRC examples.
"""
from pathlib import Path
import json
import xml.etree.ElementTree as E
import binascii

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT.parent / 'docs/06_示例与测试数据/musp-v08-reference.xml'
GOLD = ROOT.parent / 'docs/06_示例与测试数据/golden-vectors.json'
OUTPUT = ROOT / 'protocols/MUSP_v0.8_无人机联调_基座试录.xml'
source = E.parse(SOURCE).getroot()
fragments = {f.get('id'): f for f in source.find('fragments')}
gold = json.loads(GOLD.read_text(encoding='utf-8-sig'))
root = E.Element('simterm-import')
system = E.SubElement(root, 'system', name='MUSP v0.8 无人机联调（基座试录）', description='34条信令；配置请求/响应支持动态条目与长度、CRC计算，其余信令保留静态样例。尚不支持完整MUSP运行语义。')
responses = {int(c, 16) for m in source.find('messages') for c in m.get('responseCodes', '').split()}
responses.add(0x79)
groups = {}
samples = {}
warning = '【试录限制】CRC为静态样例；改任意字段后必须外部重算CRC，变长还需更新业务体/头部长度。未启用自动应答或从响应取值。'

def group_for(code):
    name = ('01 注册与认证' if code < 0x20 else '02 配置与心跳' if code < 0x40 else '03 状态与业务' if code < 0x60 else '04 无人机呼叫' if code < 0x70 else '05 资源与通信结束')
    if name not in groups:
        g = E.SubElement(system, 'group', name=name)
        h = E.SubElement(g, 'header', enabled='true')
        specs = [('同步头',2,'AA55'),('协议版本',1,'01'),('头部长度',1,'17'),('报文总长度',2,'{tlen}'),('信令码',1,'01'),('源设备地址',3,'00003B'),('目标设备地址',3,'000001'),('公共头标志位',1,'00'),('头部保留字节',1,'00'),('事务序号',4,'00000065'),('会话ID',2,'1234'),('业务体长度',2,'0000')]
        for n,l,v in specs:
            attrs = dict(name=n,len=str(l),enc='hex',value=v,section='header')
            if n == '信令码': attrs['role'] = 'type_code'
            E.SubElement(h,'field',attrs)
        groups[name] = g
    return groups[name]

defaults = {'deviceType':'01','deviceNumericId':'0000003B','bootReason':'01','capabilityFlags':'0003','configVersion':'0001','validSeconds':'00000E10','acceptedCapabilities':'0003','tokenValidSeconds':'00000E10','tokenLength':'04','token':'DEADBEEF','uptimeSeconds':'0000003C','deviceState':'01','targetDeviceAddr':'00003B','itemCount':'05','channelCount':'00','activeCallCount':'00','paramLength':'0000'}

def add_field(parent, f, prefix='', forced=None):
    key=f.get('key'); length=int(f.get('bytes', '4' if key=='token' else '0'))
    val = forced or f.get('fixed') or f.get('default') or defaults.get(key) or (f.find('enum').get('hex') if f.find('enum') is not None else '00'*length)
    attrs=dict(name=prefix+f.get('labelZh',key),len=str(max(1,length)),enc='hex',value=val if length else '{vlen}')
    enum=f.findall('enum')
    if enum:
        dk='musp_'+key+'_'+str(len(list(root)))+'_'+str(len(list(parent)))+'_'+str(len(samples))
        d=E.SubElement(root,'dict',key=dk,name=attrs['name'])
        for it in enum: E.SubElement(d,'item',value=it.get('hex'),label=it.get('labelZh'))
        attrs['dict']=dk
    return E.SubElement(parent,'field',attrs)

for m in sorted(source.find('messages'),key=lambda x:int(x.get('code'),16)):
    code=int(m.get('code'),16); g=group_for(code)
    typ='Response' if code in responses else 'Heartbeat' if code==0x31 else 'Request' if m.get('responseCodes') else 'Event'
    p=E.SubElement(g,'protocol',id='musp_'+m.get('id').lower(),name=f'0x{code:02X} '+m.get('nameZh'),type=typ,description=warning+' 默认示例仅用于字段录入、编码和原始收发检查。')
    def override(n,l,v): return E.SubElement(p,'field',name=n,len=str(l),enc='hex',value=v,**{'header-ref':n})
    override('信令码',1,f'{code:02X}').set('role','type_code')
    reverse = code in [0x02,0x04,0x11,0x21,0x32,0x51,0x61,0x62,0x64,0x66,0x70,0x71,0x74,0x75,0x76,0x78,0x7A,0x7C]
    override('源设备地址',3,'000001' if reverse else '00003B')
    override('目标设备地址',3,'00003B' if reverse else '000001')
    override('公共头标志位',1,'01' if code==0x21 else '02' if typ=='Response' else '00')
    if code==1: override('会话ID',2,'0000')
    if code==0x21:
        override('头部长度',1,'1D')
        for n,l,v in [('认证类型',1,'A1'),('认证长度',1,'04'),('认证信息（测试值）',4,'DEADBEEF')]:
            E.SubElement(p,'field',name=n,len=str(l),enc='hex',value=v,section='header_extension',**{'section-name':'认证扩展'})
    if code in (0x21,0x22):
        raw=bytes.fromhex(gold['request' if code==0x21 else 'response']['hex'])
        body=raw[raw[3]:-2]; pos=0
        def take(f,prefix=''):
            global pos
            size=int(f.get('bytes')); add_field(p,f,prefix,body[pos:pos+size].hex()); pos+=size
        if code==0x21:
            for frag in ['configCommon8','configSetPrefix10']:
                for f in fragments[frag]: take(f, '通用段·' if frag=='configCommon8' else '')
            for i in range(5):
                for f in fragments['channelItem10']: take(f,f'条目{i+1}·')
            p.set('description',warning+' 99B golden样例；模式02、5条10B记录按字段展开；动态数量/模式切换未支持，勿只改单个模式或计数。Bit7原子性、Bit0结束；其余6位保留。')
        else:
            take(m.find('body/field'))
            for i in range(5): take(m.find('body/array/field'),f'条目{i+1}·')
            p.set('description',warning+' 31B golden样例；5条AA仅表示对应项配置成功，其他值含义未定义；动态条目数和业务提交未支持。')
    else:
        for child in m.find('body'):
            if child.tag=='field': add_field(p,child)
            elif child.tag=='array':
                # Empty arrays remain explicit variable lists with unit metadata.
                unitlen=int(child.get('elementBytes','1'))
                arr=E.SubElement(p,'field',name=child.get('labelZh'),len=str(unitlen),enc='hex',value='{vlen}',role='device_list')
                for f in child:
                    u=add_field(arr,f); u.tag='unit'
                countkey=child.get('countRef','').split('.')[-1]
                fsrc=next((f for f in m.find('body') if f.get('key')==countkey),None)
                if fsrc is not None:
                    for f in p.findall('field'):
                        if f.get('name')==fsrc.get('labelZh'): f.set('role','device_count')
        body=b''.join(bytes.fromhex(f.get('value')) for f in p.findall('field') if not f.get('header-ref') and f.get('value')!='{vlen}')
    override('业务体长度',2,len(body).to_bytes(2,'big').hex())
    # Render static defaults to calculate a sample CRC, retaining independent fields.
    header={f.get('name'):dict(f.attrib) for f in g.find('header')}
    for f in p.findall('field'):
        if f.get('header-ref'): header[f.get('header-ref')]['value']=f.get('value')
    bodybytes=b''.join(bytes.fromhex(f.get('value')) for f in p.findall('field') if not f.get('header-ref') and f.get('value')!='{vlen}')
    total=23+len(bodybytes)+2
    h=b''.join(total.to_bytes(int(f['len']),'big') if f['value']=='{tlen}' else bytes.fromhex(f['value']) for f in header.values())
    crc=binascii.crc_hqx(h+bodybytes,0xffff).to_bytes(2,'big')
    E.SubElement(p,'field',name='CRC16（静态样例，修改后须重算）',len='2',enc='hex',value=crc.hex().upper(),section='trailer')
    samples[p.get('id')]=(h+bodybytes+crc).hex(' ').upper()
    if code in (0x21,0x22): assert samples[p.get('id')]==gold['request' if code==0x21 else 'response']['hex']

# Upgrade configuration messages to real repeatable records. Keep golden defaults.
import copy
for code, ident, array_name in [(0x21,'musp_config_set','通道配置条目'),(0x22,'musp_config_result','配置结果条目')]:
    p=next(p for p in system.iter('protocol') if p.get('id')==ident)
    flat=[f for f in p.findall('field') if f.get('name','').startswith('条目')]
    first=list(p).index(flat[0])
    arr=E.Element('field',name=array_name,len='10' if code==0x21 else '1',enc='hex',role='device_list',value=''.join(f.get('value') for f in flat))
    for f in flat[:6 if code==0x21 else 1]:
        u=copy.deepcopy(f);u.tag='unit';u.set('name',u.get('name').split('·',1)[1]);arr.append(u)
    for f in flat:p.remove(f)
    count=next(f for f in p.findall('field') if f.get('name')=='配置条目数');count.set('role','device_count')
    if code==0x21:
        arr.set('when-field','配置模式');arr.set('when-value','02')
        devices=E.Element('field',name='设备配置条目',len='10',enc='hex',role='device_list',value=''.join(f'{a:06X}01{a:08X}0001' for a in range(0x3b,0x40)),**{'when-field':'配置模式','when-value':'01'})
        for f in fragments['deviceItem10']:
            u=add_field(devices,f);u.tag='unit'
        p.insert(first,devices);p.insert(first+1,arr)
        target=next(f for f in p.findall('field') if f.get('name')=='目标设备地址' and not f.get('header-ref'))
        target.set('when-field','配置模式');target.set('when-value','02')
        device_target=copy.deepcopy(target);device_target.set('name','目标设备地址（设备模式）');device_target.set('value','000000');device_target.set('when-value','01')
        p.insert(list(p).index(target),device_target)
    else:p.insert(first,arr)
    crc_field=next(f for f in p.findall('field') if f.get('name','').startswith('CRC16'))
    crc_field.set('name','CRC16');crc_field.set('value','{crc16_ccitt_false}')
    length=next(f for f in p.findall('field') if f.get('header-ref')=='业务体长度')
    start='通用段·配置控制标志' if code==0x21 else '配置条目数'
    length.set('value','{byte_length:'+start+':CRC16}')
    p.set('description','配置条目数量驱动动态数组；业务体长度、报文总长度与CRC自动计算。模式01设备条目，模式02通道参数。配置结果AA表示逐项成功。')

E.indent(root,space='  ')
OUTPUT.parent.mkdir(exist_ok=True)
E.ElementTree(root).write(OUTPUT,encoding='utf-8',xml_declaration=True)
(ROOT/'protocols/MUSP_v0.8_试录静态样例.json').write_text(json.dumps(samples,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(f'{len(samples)} protocols: {OUTPUT}')
print('Golden request 99B D35D; response 31B FECB preserved. Config arrays/length/CRC are dynamic.')
