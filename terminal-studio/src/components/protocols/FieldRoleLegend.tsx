/** 字段用途说明: 分组两栏(用途名 | 说明), 点开即看每个选项干什么、怎么配、示例 */
import { HelpCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { FIELD_ROLE_OPTIONS } from "@/components/protocols/fieldRoles";

/** 展示分组: 顺序即重要性 */
const GROUPS: Array<{ title: string; roles: string[] }> = [
  {
    title: "识别 — 收到报文怎么认出这条协议",
    roles: ["type_code"],
  },
  {
    title: "认证与取值 — 从响应里拿值, 发送时自动带",
    roles: ["session", "capture", "src_id"],
  },
  {
    title: "设备批量 — 列表帧的录入与档案",
    roles: ["device_list", "device_count", "device_sn", "op_type", "result"],
  },
  {
    title: "其他",
    roles: ["dst_id", "none"],
  },
];

export function FieldRoleLegend() {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 shrink-0"
          title="字段用途说明"
        >
          <HelpCircle className="h-4 w-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        side="bottom"
        align="start"
        className="w-[640px] max-h-[70vh] overflow-y-auto"
      >
        <p className="mb-1 text-sm font-semibold">字段用途说明</p>
        <p className="mb-4 text-xs text-muted-foreground">
          在字段表的「字段用途」列下拉选择, 随时可改可清除; 选「无」= 普通数据字段, 不触发任何自动行为。
        </p>
        <div className="space-y-4">
          {GROUPS.map((g) => (
            <div key={g.title}>
              <p className="mb-1.5 text-xs font-medium text-primary/90">
                {g.title}
              </p>
              <div className="divide-y divide-border rounded-md border">
                {g.roles.map((v) => {
                  const o = FIELD_ROLE_OPTIONS.find((x) => x.value === v);
                  if (!o) return null;
                  return (
                    <div key={v} className="grid grid-cols-[7rem_1fr] gap-3 px-3 py-2">
                      <span className="pt-0.5 text-[13px] font-semibold leading-snug">
                        {o.label}
                      </span>
                      <p className="text-xs leading-relaxed text-muted-foreground">
                        {o.guide}
                      </p>
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
