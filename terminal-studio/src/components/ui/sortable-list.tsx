/**
 * @file sortable-list.tsx
 * @description 通用拖拽排序列表: DndContext 容器 + 可拖拽行(手柄触发) + DragOverlay 浮动副本
 *
 * 拖动时用 DragOverlay 渲染行的轻量副本跟随指针, 原位行保持不动只做让位动画,
 * 避免直接拖真实行(含受控输入)导致的迟滞感; 副本挂 body Portal, 不被滚动区裁剪。
 */
import * as React from "react";
import { createPortal } from "react-dom";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical } from "lucide-react";

/**
 * 拖拽排序容器: 包裹全部行, 拖拽结束后回调新顺序。
 *
 * 行 id 用索引字符串即可(行内容为受控 state, 顺序即身份)。
 *
 * @param count - 行数
 * @param onReorder - (from, to) 回调, 调用方自行 arrayMove
 * @param renderOverlay - 拖动中的浮动副本(轻量只读行), 不传则无副本
 */
export function SortableContainer({
  count,
  onReorder,
  renderOverlay,
  className,
  children,
}: {
  count: number;
  onReorder: (from: number, to: number) => void;
  renderOverlay?: (index: number) => React.ReactNode;
  className?: string;
  children: React.ReactNode;
}) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
  );
  const ids = React.useMemo(
    () => Array.from({ length: count }, (_, i) => `${i}`),
    [count],
  );
  const [activeId, setActiveId] = React.useState<string | null>(null);

  const onDragStart = (e: DragStartEvent) => {
    setActiveId(String(e.active.id));
  };
  const onDragEnd = (e: DragEndEvent) => {
    setActiveId(null);
    const { active, over } = e;
    if (!over || active.id === over.id) return;
    const from = Number(active.id);
    const to = Number(over.id);
    if (Number.isInteger(from) && Number.isInteger(to)) onReorder(from, to);
  };
  const onDragCancel = () => setActiveId(null);

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onDragCancel={onDragCancel}
    >
      <SortableContext items={ids} strategy={verticalListSortingStrategy}>
        <div className={className}>{children}</div>
      </SortableContext>
      {createPortal(
        <DragOverlay dropAnimation={{ duration: 150, easing: "ease" }}>
          {activeId ? renderOverlay?.(Number(activeId)) ?? null : null}
        </DragOverlay>,
        document.body,
      )}
    </DndContext>
  );
}

/**
 * 可拖拽行: children 为函数, 接收拖拽手柄按钮, 由调用方放进行首列。
 * 拖动中原位行降透明度让位, 视觉主体是 DragOverlay 副本。
 */
export function SortableRow({
  id,
  className,
  style,
  children,
}: {
  id: string | number;
  className?: string;
  style?: React.CSSProperties;
  children: (handle: React.ReactNode) => React.ReactNode;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });

  const handle = (
    <button
      type="button"
      ref={setActivatorNodeRef}
      title="拖拽调整顺序"
      className="cursor-grab touch-none rounded p-0.5 text-muted-foreground hover:text-foreground active:cursor-grabbing"
      {...attributes}
      {...listeners}
    >
      <GripVertical className="h-3.5 w-3.5" />
    </button>
  );

  return (
    <div
      ref={setNodeRef}
      style={{
        ...style,
        transform: CSS.Transform.toString(transform),
        transition,
        zIndex: isDragging ? 5 : undefined,
        opacity: isDragging ? 0.3 : 1,
      }}
      className={className}
    >
      {children(handle)}
    </div>
  );
}
