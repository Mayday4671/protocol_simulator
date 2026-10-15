export const NEW_PROTOCOL = `<?xml version="1.0" encoding="UTF-8"?>
<protocol id="custom-protocol" nameZh="新建协议" version="0.1.0" schemaVersion="1" byteOrder="big" maxPacketBytes="65507">
  <fragments>
    <fragment id="messageHeader" nameZh="公共信令头">
      <field key="msgCode" labelZh="信令码" bytes="1" display="HEX" valueKind="uint" binding="message.code" />
    </fragment>
  </fragments>
  <messages>
    <message id="MESSAGE_01" code="0x01" nameZh="新建信令">
      <header ref="messageHeader" />
      <body>
        <field key="value" labelZh="数据" bytes="1" valueKind="uint" display="HEX" default="00" />
      </body>
    </message>
  </messages>
</protocol>`;

