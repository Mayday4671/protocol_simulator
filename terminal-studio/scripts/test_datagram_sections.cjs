const assert = require('node:assert/strict');
const path = require('node:path');
const esbuild = require('esbuild');
const root = path.resolve(__dirname, '..');
const output = esbuild.buildSync({
  entryPoints: [path.join(root, 'src/components/monitor/datagramSections.ts')],
  absWorkingDir: root,
  bundle: true,
  format: 'cjs',
  platform: 'node',
  write: false,
}).outputFiles[0].text;
const compiled = { exports: {} };
new Function('module', 'exports', output)(compiled, compiled.exports);
const { buildDatagramSections, formatByteRange, datagramEndpoints } = compiled.exports;
const field = (name, start, len, extra = {}) => ({ name, start, len, hex: '0x00', dec: '0', meaning: '', dict_label: null, is_header: false, ...extra });
const bytes = '00112233445566778899AABBCCDDEEFF';

const ordered = buildDatagramSections([
  field('one', 0, 2, { section: 'header', section_name: 'A' }),
  field('two', 2, 1, { section: 'header', section_name: 'B' }),
  field('three', 3, 1, { section: 'header', section_name: 'A' }),
  field('four', 4, 1, { section: 'payload', field_name: 'short' }),
  field('five', 5, 1, { section: 'trailer' }),
], bytes.slice(0, 12));
assert.deepEqual(ordered.map(s => s.name), ['A', 'B', 'A', '净荷', '尾部']);
assert.deepEqual(ordered.flatMap(s => s.fields.map(f => f.name)), ['one', 'two', 'three', 'four', 'five']);
assert.deepEqual(ordered.map(s => s.byteLength), [2, 1, 1, 1, 1]);
assert.equal(ordered[3].fields[0].displayName, 'short');

const records = buildDatagramSections([
  field('items[1].a', 0, 1, { record_name: '设备', record_index: 1, field_name: 'a' }),
  field('items[1].b', 1, 1, { record_name: '设备', record_index: 1, field_name: 'b' }),
  field('items[2].a', 2, 1, { record_name: '设备', record_index: 2, field_name: 'a' }),
  field('count', 3, 1),
  field('items[1].a', 4, 1, { record_name: '设备', record_index: 1, field_name: 'a' }),
], bytes.slice(0, 10));
assert.deepEqual(records[0].groups.map(g => [g.recordName, g.recordIndex, g.fields.length]), [['设备', 1, 2], ['设备', 2, 1], [null, null, 1], ['设备', 1, 1]]);

const fallback = buildDatagramSections([
  field('header fallback', 0, 1, { is_header: true }),
  field('名字叫CRC也不能猜成尾部', 1, 1),
], '00FF');
assert.deepEqual(fallback.map(s => s.kind), ['header', 'payload']);
assert.equal(buildDatagramSections([field('explicit payload', 0, 1, { is_header: true, section: 'payload' })], '00')[0].kind, 'payload');

const gaps = buildDatagramSections([field('defined', 2, 2), field('truncated', 6, 8)], bytes.slice(0, 16));
assert.deepEqual(gaps.map(s => [s.kind, s.start, s.end, s.byteLength]), [['unparsed', 0, 2, 2], ['payload', 2, 4, 2], ['unparsed', 4, 6, 2], ['payload', 6, 8, 2]]);
assert.equal(gaps[3].fields[0].actualLength, 2);
assert.equal(gaps[3].fields[0].hex, '0x6677');
assert.equal(buildDatagramSections([], 'AABB')[0].kind, 'unparsed');
assert.deepEqual(buildDatagramSections([], ''), []);

const zero = buildDatagramSections([field('empty array', 0, 0), field('payload', 0, 2)], 'ABCD');
assert.equal(zero[0].fields.length, 2);
assert.equal(zero[0].fields[0].actualLength, 0);
assert.equal(zero[0].byteLength, 2);
assert.equal(formatByteRange(0, 0), '@0');
assert.equal(formatByteRange(2, 1), '2');
assert.equal(formatByteRange(2, 4), '2–5');
assert.deepEqual(datagramEndpoints({ direction: 'in', local_addr: 'local', remote_addr: 'remote' }), { source: 'remote', target: 'local' });
assert.deepEqual(datagramEndpoints({ direction: 'out', local_addr: 'local', remote_addr: 'remote' }), { source: 'local', target: 'remote' });
console.log('PASS: contiguous sections, A/B/A order, records, fallback, actual byte ranges, gaps, zero-length, inbound/outbound endpoints');
