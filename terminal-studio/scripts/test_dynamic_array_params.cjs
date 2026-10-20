// Exercise the same pure parameter builder used by single and periodic sends.
const assert = require('node:assert/strict');
const path = require('node:path');
const esbuild = require('esbuild');
const root = path.resolve(__dirname, '..');
const output = esbuild.buildSync({
  entryPoints: [path.join(root, 'src/lib/simulationParams.ts')],
  absWorkingDir: root,
  bundle: true,
  format: 'cjs',
  platform: 'node',
  write: false,
}).outputFiles[0].text;
const compiled = { exports: {} };
new Function('module', 'exports', output)(compiled, compiled.exports);
const { buildSimulationParams } = compiled.exports;

const field = (name, len, enc, value, role = 'none', extra = {}) => ({ name, len, enc, value, role, ...extra });
const count = field('count', 1, 'uint', '1', 'device_count');
const mode = field('mode', 1, 'hex', '01');
const units = [field('temperature', 2, 'uint', '300'), field('tag', 3, 'ascii', 'Ab')];
const list = field('samples', 5, 'hex', '{vlen}', 'device_list', { unit_fields: units, when: { field: 'mode', value: '01' } });
const hidden = field('hidden', 1, 'hex', '{vlen}', 'device_list', { unit_fields: [field('other', 1, 'hex', '00')], when: { field: 'mode', value: '02' } });
const fields = [mode, count, list, hidden, field('size', 2, 'hex', '{tlen}'), field('bodySize', 2, 'hex', '{byte_length:mode:crc}'), field('crc', 2, 'hex', '{crc16_ccitt_false}')];
const stale = { hidden: 'BB', __invalid_list_hidden: 'inactive error', size: '', bodySize: 'FFFF', crc: '0000' };
assert.deepEqual(buildSimulationParams(fields, { mode: '01', count: '1' }, stale, [], {}), { samples: '012C416200' });
assert.deepEqual(buildSimulationParams(fields, { mode: '01', count: '0' }, { ...stale, count: '0' }, [], {}), { count: '0', samples: '' });
assert.equal(buildSimulationParams(fields, { mode: '01', count: '2' }, stale, [], { samples: [{ temperature: '513', tag: 'X' }] }).samples, '0201580000012C416200');
assert.throws(() => buildSimulationParams(fields, { mode: '01', count: '-1' }, {}, [], {}), /超出/);
assert.throws(() => buildSimulationParams(fields, { mode: '01', count: '1.5' }, {}, [], {}));
assert.throws(() => buildSimulationParams(fields, { mode: '01', count: '201' }, {}, [], {}), /最多编辑 200/);
assert.throws(() => buildSimulationParams(fields, { mode: '01', count: '1' }, {}, [], { samples: [{ temperature: '300', tag: 'four' }] }), /文本超过/);
console.log('PASS: uint/ASCII arrays, send-time count validation, independent hidden errors, stale computed overrides');
