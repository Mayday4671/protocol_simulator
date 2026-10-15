import {Button,Space} from 'antd';
import {PauseOutlined,CaretRightOutlined,StepForwardOutlined,StopOutlined} from '@ant-design/icons';
import {runControls,type RunControl} from '../utils/runtime';
import {useAction} from './Shared';
const labels:Record<RunControl,string>={pause:'暂停',resume:'继续',step:'单步',stop:'停止'};
const icons={pause:<PauseOutlined/>,resume:<CaretRightOutlined/>,step:<StepForwardOutlined/>,stop:<StopOutlined/>};
export function RunControls({runId,status}:{runId:string;status:string}){
 const {perform,busy}=useAction();
 return <Space size={4}>{runControls(status).map(control=><Button key={control} size="small" loading={busy} danger={control==='stop'} icon={icons[control]} onClick={event=>{event.stopPropagation();void perform('flow_'+control,{runId},'运行控制已执行：'+labels[control])}}>{labels[control]}</Button>)}</Space>;
}
