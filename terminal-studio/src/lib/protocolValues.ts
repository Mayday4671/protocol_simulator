import type { ProtocolField } from '@/types';

export function encodeValue(f: ProtocolField, value: string): string {
  if (f.enc === 'uint' && !f.dict) {
    const n=BigInt(value || '0');
    if (n<0n || n >= (1n << BigInt(f.len*8))) throw new Error(`${f.name} 超出 ${f.len} 字节整数范围`);
    return n.toString(16).toUpperCase().padStart(f.len*2,'0');
  }
  if (f.enc === 'ascii' && !f.dict) {
    const bytes=new TextEncoder().encode(value);
    if (bytes.length>f.len) throw new Error(`${f.name} 文本超过 ${f.len} 字节`);
    return Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('').toUpperCase().padEnd(f.len*2,'0');
  }
  const hex=value.replace(/\s/g,'').toUpperCase() || '0'.repeat(f.len*2);
  if (!/^[0-9A-F]*$/.test(hex) || hex.length!==f.len*2) throw new Error(`${f.name} 需要 ${f.len} 字节 Hex`);
  return hex;
}

export function entryCount(f:ProtocolField|undefined, value:string):number {
  if (!f) throw new Error('重复分组之前缺少条目数字段');
  const hex=encodeValue(f,value);
  const n=Number(BigInt('0x'+hex));
  if(n>200) throw new Error('界面最多编辑 200 条，请减少条目数（不会静默截断）');
  return n;
}
