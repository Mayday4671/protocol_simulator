const assert = require('node:assert/strict');
const path = require('node:path');
const esbuild = require('esbuild');
const root = path.resolve(__dirname, '..');
const output = esbuild.buildSync({
  entryPoints: [path.join(root, 'src/lib/deviceProfiles.ts')],
  absWorkingDir: root,
  bundle: true,
  format: 'cjs',
  platform: 'node',
  write: false,
}).outputFiles[0].text;
const compiled = { exports: {} };
new Function('module', 'exports', output)(compiled, compiled.exports);
const { protocolFrame } = compiled.exports;

const field = (name, extra = {}) => ({ name, len: 1, enc: 'hex', value: '01', ...extra });
const header = [
  field('type', { section: 'header', section_name: '公共头 A' }),
  field('session', { section: 'header_extension', section_name: '公共头 B' }),
];
const body = [
  field('type override', { hdr_ref: 'type', section: 'header_extension', section_name: '指令扩展头', value: '02' }),
  field('session override', { hdr_ref: 'session', section: null, section_name: null }),
  field('samples', { section: 'payload', section_name: '测量值', unit_fields: [field('sample', { section_name: '采样项' })] }),
  field('checksum', { section: 'trailer', section_name: '校验尾' }),
];
const protocol = { def_mode: 'fields', fields_json: JSON.stringify(body) };
const group = { header_enabled: true, header_fields_json: JSON.stringify(header) };
const frame = protocolFrame(protocol, group);

assert.equal(frame.head[0].section, 'header_extension');
assert.equal(frame.head[0].section_name, '指令扩展头');
assert.equal(frame.head[0].value, '02');
assert.equal(frame.head[1].section, 'header_extension');
assert.equal(frame.head[1].section_name, '公共头 B');
assert.deepEqual(frame.fields.map(f => f.name), ['type', 'session', 'samples', 'checksum']);
assert.deepEqual(frame.fields.map(f => f.len), [1, 1, 1, 1]);
assert.equal(frame.body[0].unit_fields[0].section_name, '采样项');
assert.equal(frame.body[1].section, 'trailer');

const legacy = protocolFrame({ def_mode: 'fields', fields_json: JSON.stringify([field('data')]) });
assert.equal(legacy.fields[0].section, undefined);
assert.equal(legacy.fields[0].section_name, undefined);
console.log('PASS: section overrides, automatic inheritance, nested metadata, unchanged field order, legacy defaults');
