/**
 * 皮肤设置面板: 选择本地图片作全局背景, 可调透明度, 支持清除
 */
import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Eraser, ImageIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import { ACCENTS, useSkin } from "@/hooks/useSkin";

export function SkinPanel({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  const skin = useSkin();
  const [picking, setPicking] = useState(false);

  const pickImage = async () => {
    setPicking(true);
    try {
      const path = await openDialog({
        title: "选择背景图片",
        multiple: false,
        filters: [
          {
            name: "图片",
            extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
          },
        ],
      });
      if (typeof path === "string") {
        skin.setSkin(path);
        toast.success("背景已应用");
      }
    } catch (e) {
      toast.error("选择图片失败", { description: String(e) });
    } finally {
      setPicking(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm" aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>皮肤设置</DialogTitle>
        </DialogHeader>

        {/* 当前背景预览 */}
        <div className="h-28 overflow-hidden rounded-lg border bg-muted/40">
          {skin.url ? (
            <img
              src={skin.url}
              alt="背景预览"
              className="h-full w-full object-cover"
            />
          ) : (
            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
              未设置背景图片
            </div>
          )}
        </div>

        <Button onClick={() => void pickImage()} disabled={picking}>
          <ImageIcon /> {skin.skin.path ? "更换图片" : "选择图片"}
        </Button>

        <div className="grid gap-2">
          <div className="flex items-center justify-between">
            <Label className="text-xs text-muted-foreground">皮肤浓度</Label>
            <span className="font-mono text-xs text-muted-foreground">
              {Math.round(skin.skin.opacity * 100)}%
            </span>
          </div>
          <input
            type="range"
            min={5}
            max={100}
            step={5}
            value={Math.round(skin.skin.opacity * 100)}
            onChange={(e) => skin.setOpacity(Number(e.target.value) / 100)}
            className="w-full accent-primary"
            disabled={!skin.skin.path}
          />
        </div>

        {/* 主题色: 即时切换 primary 强调色 */}
        <div className="grid gap-2">
          <Label className="text-xs text-muted-foreground">主题色</Label>
          <div className="grid grid-cols-6 gap-2">
            {ACCENTS.map((a) => (
              <button
                key={a.id}
                type="button"
                title={a.name}
                onClick={() => skin.setAccent(a.id)}
                className={cn(
                  "h-7 w-7 rounded-full border-2 transition-transform hover:scale-110",
                  skin.accentId === a.id
                    ? "border-foreground/80 scale-110"
                    : "border-transparent",
                )}
                style={{
                  backgroundColor: `hsl(${a.hsl[0]} ${a.hsl[1]}% ${a.hsl[2]}%)`,
                }}
              />
            ))}
          </div>
        </div>

        {skin.skin.path && (
          <Button
            variant="outline"
            onClick={() => {
              skin.setSkin(null);
              toast.info("已恢复默认背景");
            }}
          >
            <Eraser /> 恢复默认
          </Button>
        )}
      </DialogContent>
    </Dialog>
  );
}
