import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { FieldSection, ProtocolField } from "@/types";

const AUTO_SECTION = "__auto__";
const SECTION_OPTIONS: { value: FieldSection; label: string }[] = [
  { value: "header", label: "公共头" },
  { value: "header_extension", label: "扩展头" },
  { value: "payload", label: "净荷" },
  { value: "trailer", label: "尾部" },
];

export function FieldSectionSettings({
  field,
  onChange,
}: {
  field: Pick<ProtocolField, "name" | "section" | "section_name">;
  onChange: (patch: Pick<ProtocolField, "section" | "section_name">) => void;
}) {
  return (
    <>
      <label className="flex items-center gap-2 whitespace-nowrap text-xs">
        所属分区
        <Select
          value={field.section ?? AUTO_SECTION}
          onValueChange={(value) =>
            onChange({ section: value === AUTO_SECTION ? null : value as FieldSection })
          }
        >
          <SelectTrigger
            aria-label={`所属分区：${field.name}`}
            title="报文详情中的显示分区"
            className="h-8 w-44 text-xs"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={AUTO_SECTION}>自动</SelectItem>
            {SECTION_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <label className="flex items-center gap-2 whitespace-nowrap text-xs">
        分区名称
        <Input
          aria-label={`分区名称：${field.name}`}
          className="h-8 max-w-xs text-xs"
          placeholder="可选，如公共头 A"
          value={field.section_name ?? ""}
          onChange={(event) => onChange({ section_name: event.target.value })}
        />
      </label>
    </>
  );
}
