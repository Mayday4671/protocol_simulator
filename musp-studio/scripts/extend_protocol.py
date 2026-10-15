"""Build the bundled editable sample from MUSP v0.8 tables (never from screenshots).
Source: docs/07_原始协议讨论稿, sections 5, 7, 7A, 7B, 7C.
The original two-message XML and input documents remain unchanged.
"""
from pathlib import Path
import xml.etree.ElementTree as E

root = Path(__file__).resolve().parents[1]
tree = E.parse(root / 'protocols/musp-demo.xml')
protocol = tree.getroot()
protocol.set('id', 'musp-v08')
protocol.set('nameZh', 'MUSP v0.8 业务样例')
messages = protocol.find('messages')
# code, id, label, fixed body field widths. All numeric values are supplied at runtime.
definitions = [
 (0x01,'REGISTER_REQ','开机注册请求','deviceType:1,deviceNumericId:4,bootReason:1,capabilityFlags:2,configVersion:2'),
 (0x02,'REGISTER_LICENSE','注册许可','resultCode:1,validSeconds:4,authMode:1,reserved:1'),
 (0x03,'REGISTER_CONFIRM','注册确认','deviceNumericId:4,confirmResult:1'),
 (0x04,'REGISTER_COMPLETE','注册完成','resultCode:1,acceptedCapabilities:2'),
 (0x11,'AUTH_ISSUE','认证信息下发','tokenValidSeconds:4,tokenLength:1,token:tokenLength'),
 (0x12,'AUTH_ACK','认证接收确认','resultCode:1'),
 (0x31,'HEARTBEAT_REQ','心跳请求','uptimeSeconds:4,statusFlags:2'),
 (0x32,'HEARTBEAT_RSP','心跳响应','resultCode:1,serverTimeSeconds:4'),
 (0x41,'STATUS_REPORT','状态上报','deviceState:1,channelCount:1,voltageMv:2,temperatureCentiC:2'),
 (0x51,'BUSINESS_CMD','业务命令','commandType:1,paramLength:2,params:paramLength'),
 (0x52,'BUSINESS_RESULT','业务结果','resultCode:1,errorCode:2'),
 (0x60,'VIDEO_CALL_REQ','视频通话申请','calleeAddr:3,mediaType:1,requestTxKbps:2,requestRxKbps:2,codecFlags:2,priority:1,reserved:1'),
 (0x61,'VIDEO_CALL_RSP','视频通话响应','resultCode:1,callId:2,mediaType:1,callState:1,reserved:1'),
 (0x62,'PAGING_REQ','寻呼请求','callerAddr:3,callId:2,mediaType:1,suggestTxKbps:2,suggestRxKbps:2,responseTimeoutSec:2,reserved:1'),
 (0x63,'PAGING_RSP','寻呼响应','callId:2,pagingResult:1,acceptedMedia:1,reserved:1'),
 (0x64,'CALL_NOTIFY','通话许可通知','callId:2,resultCode:1,peerAddr:3'),
 (0x65,'VOICE_CALL_REQ','单端语音申请','serviceType:1,requestTxKbps:2,requestRxKbps:2,codecType:1,priority:1,reserved:1'),
 (0x66,'VOICE_CALL_RSP','语音通话响应','resultCode:1,callId:2,callState:1,reserved:1'),
 (0x70,'RX_RESOURCE_CFG','接收资源配置','callId:2,resourceId:2,assignedRxKbps:2,rxFreqKHz:4,slotIndex:2,resourceVersion:2,applyFlag:1'),
 (0x71,'TX_RESOURCE_CFG','发送资源配置','callId:2,resourceId:2,assignedTxKbps:2,txFreqKHz:4,slotIndex:2,resourceVersion:2,applyFlag:1'),
 (0x72,'RESOURCE_CFG_ACK','资源配置确认','callId:2,resourceId:2,direction:1,resultCode:1,reserved:1'),
 (0x73,'CALL_RELEASE_REQ','通话结束申请','callId:2,reasonCode:1'),
 (0x74,'CALL_RELEASE_RSP','通话结束受理','callId:2,resultCode:1'),
 (0x75,'CALL_READY_NOTIFY','通话就绪通知','callId:2,eventType:1,readyCode:1'),
 (0x76,'RESOURCE_RELEASE_CFG','资源撤销配置','callId:2,resourceId:2,direction:1,resourceVersion:2,releaseReason:1'),
 (0x77,'RESOURCE_RELEASE_ACK','资源撤销确认','callId:2,resourceId:2,direction:1,resourceVersion:2,resultCode:1'),
 (0x78,'CALL_END_NOTIFY','通话终止通知','callId:2,reasonCode:1,initiatorAddr:3,stopMode:1'),
 (0x79,'CALL_EVENT_ACK','通话事件确认','callId:2,eventType:1,resultCode:1'),
 (0x7A,'CALL_END_RESULT','通话结束结果','callId:2,resultCode:1,unknownCount:1'),
 (0x7B,'CALL_CANCEL_REQ','取消通话申请','originalTransactionId:4,cancelReason:1'),
 (0x7C,'CALL_CANCEL_RSP','取消申请响应','resultCode:1,reserved:1'),
 (0x7D,'DEVICE_OFFLINE_NOTIFY','设备主动离线','offlineReason:1,offlineMode:1,activeCallCount:1'),
]
# Read labels from the original field tables; no inferred wire data.
import re
source=(root.parent/'docs/07_原始协议讨论稿/MUSP_v0.8_协议业务基线_原文不修改.md').read_text(encoding='utf-8')
labels={}
for zh,key in re.findall(r'\|\s*\d+\s*\|\s*([^|（]+)（([A-Za-z][A-Za-z0-9]*)）\s*\|',source):
    labels.setdefault(key,zh.strip())
responses={1:2,3:4,0x11:0x12,0x31:0x32,0x51:0x52,0x60:0x61,0x62:0x63,0x65:0x66,0x70:0x72,0x71:0x72,0x73:0x74,0x76:0x77,0x7B:0x7C}
def field(parent,key,width):
    attrs={'key':key,'labelZh':labels.get(key,key),'display':'HEX'}
    attrs['bytes' if width.isdigit() else 'lengthExpr']=width
    if key=='reserved': attrs['fixed']='00'
    if key in ('token','params'): attrs['valueKind']='bytes'
    if key=='token': attrs['secret']='true'
    if key in ('temperatureCentiC','level','freqOffsetHz'): attrs['valueKind']='int'
    return E.SubElement(parent,'field',attrs)
for code,id,label,fields in definitions:
    msg=E.SubElement(messages,'message',{'code':f'0x{code:02X}','id':id,'nameZh':label})
    if code in responses: msg.set('responseCodes',f'0x{responses[code]:02X}')
    E.SubElement(msg,'header',{'ref':'muspHeader23'})
    E.SubElement(msg,'headerExtension',{'ref':'authTLV','when':'bit(header.flags,0)==1'})
    body=E.SubElement(msg,'body')
    for part in fields.split(','): field(body,*part.split(':'))
    if code==0x41:
        arr=E.SubElement(body,'array',{'key':'channels','labelZh':'通道状态','countRef':'body.channelCount','elementBytes':'6','maxItems':'255'})
        for key,width in [('channelId','1'),('level','2'),('freqOffsetHz','2'),('enabled','1')]:field(arr,key,width)
    if code==0x7D:
        arr=E.SubElement(body,'array',{'key':'callIds','labelZh':'通话标识列表','countRef':'body.activeCallCount','elementBytes':'2','maxItems':'255'})
        field(arr,'callId','2')
    E.SubElement(msg,'trailer',{'ref':'crcCcittFalse2'})
E.indent(tree,space='  ')
tree.write(root/'protocols/musp-v08.xml',encoding='utf-8',xml_declaration=True)
print(f'Generated {len(messages)} editable messages from documented MUSP v0.8 tables.')
