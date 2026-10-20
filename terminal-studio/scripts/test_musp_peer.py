import json, unittest
from pathlib import Path
from musp_peer import Peer, decode, encode
S=json.loads((Path(__file__).resolve().parents[1]/'protocols/MUSP_v0.8_试录静态样例.json').read_text(encoding='utf-8'))
class PeerTests(unittest.TestCase):
 def test_config_golden_and_dynamic_correlation(self):
  p=Peer(); req=bytes.fromhex(S['musp_config_set'])
  self.assertEqual(p.reply(req),bytes.fromhex(S['musp_config_result']))
  f=decode(req); f.txn=777; f.sid=4321; f.src=0x112233; f.dst=0x445566
  body=bytearray(f.body[:28]); body[10]=1; f.body=bytes(body)
  r=decode(p.reply(encode(f)))
  self.assertEqual((r.txn,r.sid,r.src,r.dst,r.body),(777,4321,0x445566,0x112233,b'\x01\xaa'))
 def test_reject_bad_crc_and_count(self):
  p=Peer(); b=bytearray.fromhex(S['musp_config_set']); b[-1]^=1
  with self.assertRaisesRegex(ValueError,'CRC'): p.reply(bytes(b))
  f=decode(bytes.fromhex(S['musp_config_set'])); f.body=f.body[:-1]
  with self.assertRaisesRegex(ValueError,'条目'): p.reply(encode(f))
 def test_registration_and_heartbeat(self):
  p=Peer(); req=bytes.fromhex(S['musp_register_req']); license=p.reply(req)
  self.assertEqual(p.reply(req),license)
  sid=decode(license).sid
  confirm=decode(bytes.fromhex(S['musp_register_confirm'])); confirm.sid=sid
  with self.assertRaisesRegex(ValueError,'新事务'): p.reply(encode(confirm))
  hb=decode(bytes.fromhex(S['musp_heartbeat_req'])); hb.sid=sid
  with self.assertRaisesRegex(ValueError,'注册'): p.reply(encode(hb))
  confirm.txn+=1; self.assertEqual(decode(p.reply(encode(confirm))).code,4)
  hb.txn+=2; r=decode(p.reply(encode(hb))); self.assertEqual(r.code,0x32); self.assertEqual(r.body[0],0)
  self.assertGreater(int.from_bytes(r.body[1:],'big'),0)
  hb.sid+=1
  with self.assertRaisesRegex(ValueError,'会话'): p.reply(encode(hb))
 def test_unsupported_is_not_success(self):
  with self.assertRaisesRegex(ValueError,'未支持'): Peer().reply(bytes.fromhex(S['musp_video_call_req']))
if __name__=='__main__': unittest.main()
