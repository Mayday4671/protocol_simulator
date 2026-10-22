import struct
import unittest
from iot_walkthrough_peer import HEADER, Platform


class PeerTest(unittest.TestCase):
    def test_state_and_failures(self):
        peer = Platform()
        address = ('127.0.0.1', 39171)
        def exchange(code, body, token=0):
            packet = HEADER.pack(b'\xc0\xde', 1, code, 1, 0, 37, token, 14 + len(body)) + body
            answer = peer.reply(packet, address)
            header = HEADER.unpack_from(answer)
            self.assertEqual(header[2:6], (code | 0x80, 0, 1, 37))
            self.assertEqual(header[-1], len(answer))
            return header[6], answer[14:]
        self.assertEqual(exchange(0x11, b'\x01')[1][0], 3)
        token, body = exchange(1, bytes.fromhex('01010001020304'))
        self.assertNotEqual(token, 0)
        self.assertEqual(body[0], 0)
        self.assertEqual(exchange(0x11, b'\x01', 0)[1][0], 4)
        self.assertEqual(exchange(0x10, struct.pack('!HH', 25, 300), token)[1], b'\x00\x00\x19')
        self.assertEqual(exchange(0x10, struct.pack('!HH', 0, 300), token)[1][0], 2)
        self.assertEqual(exchange(0x20, b'\x01', token)[1], b'\x00\x01')
        self.assertEqual(exchange(0x11, b'\x01', token)[1], bytes.fromhex('0000FA01F4010019'))
        self.assertEqual(exchange(0x20, b'\x02', token)[1][0], 2)
        self.assertEqual(exchange(0x7F, b'\x00', token)[1], b'\x00')
        self.assertEqual(exchange(0x11, b'\x01', token)[1][0], 3)
        self.assertIsNone(peer.reply(b'bad', address))
        invalid = HEADER.pack(b'\xc0\xde', 1, 1, 1, 0, 1, 0, 20) + bytes(7)
        self.assertIsNone(peer.reply(invalid, address))


if __name__ == '__main__':
    unittest.main()
