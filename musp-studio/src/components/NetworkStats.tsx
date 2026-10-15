import type {Snapshot} from '../api/tauri';
export function NetworkStats({data}:{data?:Snapshot}){
 const metrics=[['RX 接收',data?.stats.rx],['TX 发送',data?.stats.tx],['队列丢包',data?.stats.queueDropped],['处理失败',data?.stats.processingErrors??data?.processingErrors],['已处理报文',data?.stats.processedPackets??data?.processedPackets]] as const;
 return <div className="network-stats">{metrics.map(([label,value],index)=><div key={label} className={index>=2&&index<=3&&!!value?'has-errors':''}><span>{label}</span><strong>{value??'—'}</strong></div>)}</div>;
}
