export interface SourceNode {tag:string;attrs:Record<string,string>;start:number;openEnd:number;closeStart:number;end:number;depth:number;selfClosing:boolean;parent?:number}
const escaped=(value:string)=>value.replace(/&/g,'&amp;').replace(/"/g,'&quot;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
const decode=(value:string)=>value.replace(/&quot;/g,'"').replace(/&lt;/g,'<').replace(/&gt;/g,'>').replace(/&amp;/g,'&');
export function parseSource(xml:string):SourceNode[]{
 const nodes:SourceNode[]=[];const stack:number[]=[];
 const tokens=/<!--[\s\S]*?-->|<\?[\s\S]*?\?>|<!\[CDATA\[[\s\S]*?\]\]>|<\/?[A-Za-z_][\w:.-]*(?:"[^"]*"|'[^']*'|[^'">])*>/g;
 for(const match of xml.matchAll(tokens)){
  const text=match[0],start=match.index!;if(text.startsWith('<!')||text.startsWith('<?'))continue;
  const tag=/^<\/?([\w:.-]+)/.exec(text)![1];
  if(text.startsWith('</')){const index=stack.pop();if(index===undefined||nodes[index].tag!==tag)throw new Error('XML 标签未闭合，先修正源码。');nodes[index].closeStart=start;nodes[index].end=start+text.length;continue;}
  const attrs:Record<string,string>={};for(const attr of text.matchAll(/([\w:.-]+)\s*=\s*(["'])(.*?)\2/gs))attrs[attr[1]]=decode(attr[3]);
  const selfClosing=/\/\s*>$/.test(text);nodes.push({tag,attrs,start,openEnd:start+text.length,closeStart:start+text.length,end:start+text.length,depth:stack.length,selfClosing,parent:stack.at(-1)});if(!selfClosing)stack.push(nodes.length-1);
 }
 if(stack.length)throw new Error('XML 标签未闭合，先修正源码。');return nodes;
}
export function patchAttributes(xml:string,node:SourceNode,values:Record<string,string>):string{
 let opening=xml.slice(node.start,node.openEnd);for(const [key,value]of Object.entries(values)){
  if(!/^[\w:.-]+$/.test(key))throw new Error('属性名无效');const pattern=new RegExp(`(\\s${key}\\s*=\\s*)(["'])(.*?)\\2`,'s');
  if(pattern.test(opening))opening=opening.replace(pattern,(_all,prefix:string)=>`${prefix}"${escaped(value)}"`);else opening=opening.replace(/\s*(\/?>)$/,` ${key}="${escaped(value)}"$1`);
 }return xml.slice(0,node.start)+opening+xml.slice(node.openEnd);
}
export function cloneNode(xml:string,node:SourceNode):string{
 return cloneNodes(xml,node?[node]:[]);
}
export function cloneNodes(xml:string,selected:SourceNode[]):string{
 const copy=copyStructure(xml,selected);const end=Math.max(...selected.map(node=>node.end));return xml.slice(0,end)+'\n'+remapStructure(xml,copy.xml)+xml.slice(end);
}
export interface StructureClipboard{version:1;xml:string}
export function copyStructure(xml:string,selected:SourceNode[]):StructureClipboard{
 const chosen=selected.filter(node=>!selected.some(parent=>parent.start<node.start&&parent.end>=node.end)).sort((a,b)=>a.start-b.start);
 if(!chosen.length)throw new Error('请先选择字段或结构');
 if(chosen.some(node=>node.parent!==chosen[0].parent))throw new Error('批量复制请选择同一容器中的字段');
 return {version:1,xml:chosen.map(node=>xml.slice(node.start,node.end)).join('\n')};
}
function remapStructure(xml:string,snippet:string):string{
 let cloned=snippet;const local=parseSource(cloned);const all=parseSource(xml);const used=new Set(all.flatMap(n=>[n.attrs.key,n.attrs.id]).filter(Boolean));const mapping=new Map<string,string>();
 for(const child of local){for(const attr of ['key','id']){const old=child.attrs[attr];if(old&&!mapping.has(old)){let name=old+'_copy',number=2;while(used.has(name))name=old+'_copy'+number++;used.add(name);mapping.set(old,name);}}}
 for(const child of [...local].reverse()){
  const values:Record<string,string>={};for(const [key,value]of Object.entries(child.attrs)){
   if(key==='key'||key==='id')values[key]=mapping.get(value)||value;
   else if(['ref','countRef','elementSizeRef','lengthExpr','countExpr','when','selector'].includes(key))values[key]=value.replace(/[A-Za-z_][A-Za-z0-9_.]*/g,path=>/^(device|session|system|response|flow|step)\./.test(path)?path:path.split('.').map(segment=>mapping.get(segment)||segment).join('.'));
  }cloned=patchAttributes(cloned,child,values);
 }
 const codes=new Set(all.filter(n=>n.tag==='message').map(n=>Number(n.attrs.code)));
 for(const node of parseSource(cloned).filter(n=>n.tag==='message').reverse()){let code=1;while(codes.has(code)&&code<256)code++;if(code===256)throw new Error('信令码空间已满');codes.add(code);cloned=patchAttributes(cloned,node,{code:'0x'+code.toString(16).padStart(2,'0').toUpperCase()});}
 return cloned;
}
export function pasteStructure(xml:string,target:SourceNode,clipboard:StructureClipboard):string{
 if(clipboard.version!==1)throw new Error('结构剪贴板版本不支持');
 const copied=parseSource(clipboard.xml);const roots=copied.filter(node=>node.parent===undefined);
 if(!roots.length)throw new Error('结构剪贴板为空');
 const requiredContainer=roots.every(n=>n.tag==='message')?'messages':roots.every(n=>n.tag==='fragment')?'fragments':undefined;
 if(requiredContainer?target.tag!==requiredContainer:!['body','fragment','array','struct'].includes(target.tag))throw new Error('粘贴位置不兼容，请选择相应的信令或共享结构容器');
 const fragments=new Set([...parseSource(xml),...copied].filter(node=>node.tag==='fragment').map(node=>node.attrs.id));
 const missing=[...new Set(copied.filter(node=>node.attrs.ref&&!fragments.has(node.attrs.ref)).map(node=>node.attrs.ref))];
 if(missing.length)throw new Error('目标协议缺少共享片段：'+missing.join('、')+'。请先复制对应 fragment。');
 return insertChild(xml,target,remapStructure(xml,clipboard.xml));
}
export function insertChild(xml:string,node:SourceNode,child:string):string{
 if(node.selfClosing){const open=xml.slice(node.start,node.openEnd).replace(/\s*\/\s*>$/, '>');return xml.slice(0,node.start)+open+'\n  '+child+'\n</'+node.tag+'>'+xml.slice(node.end);}
 return xml.slice(0,node.closeStart)+'\n  '+child+'\n'+xml.slice(node.closeStart);
}
export function insertReference(xml:string,node:SourceNode,ref:string):string{return insertChild(xml,node,`<use ref="${escaped(ref)}" />`);}
export function normalizeAddress(value:string):string{const clean=value.replace(/\s/g,'');if(!/^[a-f\d]{6}$/i.test(clean))throw new Error('协议地址必须为 3 字节 HEX，例如 00 00 3B');return clean.toUpperCase().match(/../g)!.join(' ');}
export function toggleBit(hex:string,bit:number):string{if(!/^[a-f\d]{2}$/i.test(hex)||bit<0||bit>7||!Number.isInteger(bit))throw new Error('请输入 1 字节 HEX 和 0–7 的位序号');return (parseInt(hex,16)^(1<<bit)).toString(16).padStart(2,'0').toUpperCase();}
export function moveNode(xml:string,node:SourceNode,direction:'up'|'down'):string{
 const siblings=parseSource(xml).filter(candidate=>candidate.parent===node.parent);const index=siblings.findIndex(candidate=>candidate.start===node.start);const other=siblings[index+(direction==='up'?-1:1)];if(!other)return xml;
 const [first,last]=node.start<other.start?[node,other]:[other,node];return xml.slice(0,first.start)+xml.slice(last.start,last.end)+xml.slice(first.end,last.start)+xml.slice(first.start,first.end)+xml.slice(last.end);
}
export function moveNodeTo(xml:string,node:SourceNode,target:SourceNode):string{
 if(node.parent!==target.parent)throw new Error('只能在同一容器中拖动排序');
 if(node.start===target.start)return xml;
 const snippet=xml.slice(node.start,node.end);
 if(node.start<target.start)return xml.slice(0,node.start)+xml.slice(node.end,target.end)+snippet+xml.slice(target.end);
 return xml.slice(0,target.start)+snippet+xml.slice(target.start,node.start)+xml.slice(node.end);
}

