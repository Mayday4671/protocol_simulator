import { TerminalProfiles } from '@/components/TerminalProfiles';
import { BusinessRecords } from '@/components/BusinessRecords';
import { useState } from 'react';
import { Button } from '@/components/ui/button';

export function DeviceManager() {
  const [tab, setTab] = useState('device');
  return <div className="h-full min-h-0 p-5 flex flex-col gap-3"><div className="flex gap-2"><Button size="sm" variant={tab === 'device' ? 'default' : 'outline'} onClick={() => setTab('device')}>设备档案</Button><Button size="sm" variant={tab === 'records' ? 'default' : 'outline'} onClick={() => setTab('records')}>业务资料</Button></div><div className="min-h-0 flex-1">{tab === 'device' ? <TerminalProfiles /> : <BusinessRecords />}</div></div>;
}
