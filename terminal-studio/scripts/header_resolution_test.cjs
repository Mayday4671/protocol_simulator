// Exercise the production header resolver, without UI or transport mocks.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require('typescript');
const output = ts.transpileModule(fs.readFileSync('src/lib/protocolHeaders.ts', 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const moduleExports = {};
vm.runInNewContext(output, { exports: moduleExports });
const { groupHeaderDefinitions, protocolHeaderRefs, resolveProtocolHeaders, mergeResolvedHeaderFields, packetSchemaKey } = moduleExports;
const f = (name, value) => ({ name, value, len: 1, enc: 'hex' });
const group = { name: '测试分组', header_enabled: true, header_fields_json: JSON.stringify([f('Type', '01')]) };
const refs = ids => ({ header_refs_json: JSON.stringify(ids) });
const values = fields => fields.map(field => field.value).join(',');
assert.equal(groupHeaderDefinitions(group)[0].id, 'legacy-default');
assert.equal(protocolHeaderRefs({}, group).join(','), 'legacy-default');
assert.equal(resolveProtocolHeaders(refs([]), group).length, 0);
assert.equal(resolveProtocolHeaders({}, { ...group, header_enabled: false }).length, 0);
const multi = { ...group, header_profiles_json: JSON.stringify([
  { id: 'legacy-default', name: '原公共头', fields: [f('Type', '01')] },
  { id: 'A', name: '公共头 A', fields: [f('Type', 'A1')] },
  { id: 'B', name: '公共头 B', fields: [f('Type', 'B1')] },
]) };
assert.equal(values(resolveProtocolHeaders({}, multi)), '01');
for (const [ids, expected] of [[['A'], 'A1'], [['B'], 'B1'], [['A','B'], 'A1,B1'], [['B','A'], 'B1,A1'], [[], '']]) {
  assert.equal(values(resolveProtocolHeaders(refs(ids), multi)), expected);
}
const head = resolveProtocolHeaders(refs(['A','B']), multi);
assert.equal(head[0].section_name, '公共头 A');
assert.equal(packetSchemaKey(head), packetSchemaKey(head.map(field => ({ ...field, section_name: '仅改显示名' }))));
assert.notEqual(packetSchemaKey(head), packetSchemaKey([...head].reverse()));
const merged = mergeResolvedHeaderFields(head, [{ ...f('override', 'FF'), hdr_ref: 'B::Type' }, f('Body', 'CC')]);
assert.equal(values(merged.fields), 'A1,FF,CC');
assert.equal(merged.fields.map(field => field.name).join(','), 'Type,Type#2,Body');
assert.equal(merged.head[1].header_key, 'B::Type');
assert.throws(() => resolveProtocolHeaders(refs(['missing']), multi), /不存在/);
assert.throws(() => resolveProtocolHeaders(refs(['A','A']), multi), /重复/);
assert.throws(() => mergeResolvedHeaderFields(head, [{ ...f('override', 'FF'), hdr_ref: 'Type' }]), /重名/);
assert.throws(() => mergeResolvedHeaderFields([], [{ ...f('override', 'FF'), hdr_ref: 'A::Type' }]), /未选用/);
assert.throws(() => mergeResolvedHeaderFields(head, [1,2].map(n => ({ ...f(`override${n}`, 'FF'), hdr_ref: 'A::Type' }))), /重复/);
assert.equal(values(mergeResolvedHeaderFields([head[0]], [{ ...f('old', 'AA'), hdr_ref: 'Type' }]).fields), 'AA');
const oldDuplicates = { ...group, header_fields_json: JSON.stringify([f('保留','01'), f('保留','02'), f('保留#2','03')]) };
const duplicateFields = resolveProtocolHeaders({}, oldDuplicates);
assert.equal(duplicateFields.map(field => field.header_key).join(','), 'legacy-default::保留,legacy-default::保留#3,legacy-default::保留#2');
assert.equal(values(mergeResolvedHeaderFields(duplicateFields, [{...f('旧覆盖','FF'),hdr_ref:'保留'}]).head), 'FF,02,03');
assert.equal(mergeResolvedHeaderFields(duplicateFields, []).fields.map(field => field.name).join(','), '保留,保留#3,保留#2');
assert.equal(mergeResolvedHeaderFields([{...head[0],dict:'states'}], [{...f('override','FF'),dict:'',hdr_ref:'A::Type'}]).head[0].dict, 'states');
console.log('PASS: legacy compatibility, A/B/A+B/B+A/none, provenance, qualified overrides, invalid references');
