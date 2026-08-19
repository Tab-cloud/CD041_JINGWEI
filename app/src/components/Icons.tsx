import type { SVGProps } from "react";

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, "children"> {
  size?: number;
}

type IconNode = readonly [tag: "path" | "circle" | "rect" | "line" | "polyline", attributes: Record<string, string | number>];

function createIcon(name: string, nodes: readonly IconNode[]) {
  return function Icon({ size = 18, ...props }: IconProps) {
    return (
      <svg
        aria-hidden={props["aria-label"] ? undefined : true}
        fill="none"
        height={size}
        role={props["aria-label"] ? "img" : undefined}
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
        viewBox="0 0 24 24"
        width={size}
        {...props}
      >
        <title>{props["aria-label"] ?? name}</title>
        {nodes.map(([tag, attributes], index) => {
          const Tag = tag;
          return <Tag key={`${name}-${index}`} {...attributes} />;
        })}
      </svg>
    );
  };
}

export const CircuitBoardIcon = createIcon("电路板", [
  ["rect", { x: 3, y: 3, width: 18, height: 18, rx: 2 }],
  ["path", { d: "M8 3v4a2 2 0 0 1-2 2H3" }],
  ["path", { d: "M16 3v4a2 2 0 0 0 2 2h3" }],
  ["path", { d: "M8 21v-4a2 2 0 0 0-2-2H3" }],
  ["path", { d: "M16 21v-4a2 2 0 0 1 2-2h3" }],
  ["rect", { x: 8, y: 8, width: 8, height: 8, rx: 1 }],
]);
export const MessageSquareIcon = createIcon("对话", [["path", { d: "M21 15a4 4 0 0 1-4 4H8l-5 3V7a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4z" }]]);
export const PlayIcon = createIcon("运行", [["path", { d: "m7 4 13 8-13 8z" }]]);
export const StopIcon = createIcon("停止", [["rect", { x: 5, y: 5, width: 14, height: 14, rx: 2 }]]);
export const UndoIcon = createIcon("撤销", [["path", { d: "M9 7 4 12l5 5" }], ["path", { d: "M20 17a8 8 0 0 0-13-5" }]]);
export const RedoIcon = createIcon("重做", [["path", { d: "m15 7 5 5-5 5" }], ["path", { d: "M4 17a8 8 0 0 1 13-5" }]]);
export const SaveIcon = createIcon("检查点", [["path", { d: "M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z" }], ["path", { d: "M17 21v-8H7v8" }], ["path", { d: "M7 3v5h8" }]]);
export const SearchIcon = createIcon("搜索", [["circle", { cx: 11, cy: 11, r: 7 }], ["path", { d: "m20 20-3.5-3.5" }]]);
export const ChevronRightIcon = createIcon("展开", [["path", { d: "m9 18 6-6-6-6" }]]);
export const ChevronDownIcon = createIcon("收起", [["path", { d: "m6 9 6 6 6-6" }]]);
export const LayersIcon = createIcon("图层", [["path", { d: "m12 2 9 5-9 5-9-5z" }], ["path", { d: "m3 12 9 5 9-5" }], ["path", { d: "m3 17 9 5 9-5" }]]);
export const MousePointerIcon = createIcon("选择", [["path", { d: "m3 3 7.1 17 2.4-6.5L19 11z" }]]);
export const MoveIcon = createIcon("移动", [["path", { d: "M5 9 2 12l3 3" }], ["path", { d: "m9 5 3-3 3 3" }], ["path", { d: "m15 19-3 3-3-3" }], ["path", { d: "m19 9 3 3-3 3" }], ["path", { d: "M2 12h20M12 2v20" }]]);
export const RouteIcon = createIcon("走线", [["circle", { cx: 6, cy: 19, r: 2 }], ["circle", { cx: 18, cy: 5, r: 2 }], ["path", { d: "M8 19h5a3 3 0 0 0 3-3V8" }]]);
export const ViaIcon = createIcon("过孔", [["circle", { cx: 12, cy: 12, r: 8 }], ["circle", { cx: 12, cy: 12, r: 3 }]]);
export const LinkIcon = createIcon("连接", [["path", { d: "M10 13a5 5 0 0 0 7.1.1l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1" }], ["path", { d: "M14 11a5 5 0 0 0-7.1-.1l-2 2A5 5 0 0 0 12 20l1.1-1.1" }]]);
export const LockIcon = createIcon("锁定", [["rect", { x: 4, y: 10, width: 16, height: 11, rx: 2 }], ["path", { d: "M8 10V7a4 4 0 0 1 8 0v3" }]]);
export const UnlockIcon = createIcon("解锁", [["rect", { x: 4, y: 10, width: 16, height: 11, rx: 2 }], ["path", { d: "M8 10V7a4 4 0 0 1 7.8-1" }]]);
export const AlertCircleIcon = createIcon("警告", [["circle", { cx: 12, cy: 12, r: 10 }], ["line", { x1: 12, y1: 8, x2: 12, y2: 12 }], ["line", { x1: 12, y1: 16, x2: 12.01, y2: 16 }]]);
export const CheckCircleIcon = createIcon("完成", [["circle", { cx: 12, cy: 12, r: 10 }], ["path", { d: "m8 12 3 3 5-6" }]]);
export const XIcon = createIcon("关闭", [["path", { d: "m18 6-12 12M6 6l12 12" }]]);
export const ClockIcon = createIcon("等待", [["circle", { cx: 12, cy: 12, r: 9 }], ["path", { d: "M12 7v5l3 2" }]]);
export const BotIcon = createIcon("Agent", [["rect", { x: 4, y: 6, width: 16, height: 13, rx: 3 }], ["path", { d: "M12 2v4" }], ["circle", { cx: 9, cy: 12, r: 1 }], ["circle", { cx: 15, cy: 12, r: 1 }], ["path", { d: "M9 16h6" }]]);
export const UserIcon = createIcon("用户", [["circle", { cx: 12, cy: 8, r: 4 }], ["path", { d: "M4 21a8 8 0 0 1 16 0" }]]);
export const PlusIcon = createIcon("新增", [["path", { d: "M12 5v14M5 12h14" }]]);
export const FolderOpenIcon = createIcon("打开项目", [["path", { d: "M3 6h6l2 2h10l-2 11H4z" }], ["path", { d: "M3 6V4h6l2 2" }]]);
export const ZoomInIcon = createIcon("放大", [["circle", { cx: 10, cy: 10, r: 6 }], ["path", { d: "m14.5 14.5 5 5M10 7v6M7 10h6" }]]);
export const ZoomOutIcon = createIcon("缩小", [["circle", { cx: 10, cy: 10, r: 6 }], ["path", { d: "m14.5 14.5 5 5M7 10h6" }]]);
export const CrosshairIcon = createIcon("适合窗口", [["circle", { cx: 12, cy: 12, r: 4 }], ["path", { d: "M12 2v4M12 18v4M2 12h4M18 12h4" }]]);
export const GitCompareIcon = createIcon("语义差异", [["circle", { cx: 6, cy: 5, r: 2 }], ["circle", { cx: 18, cy: 19, r: 2 }], ["path", { d: "M6 7v12M18 5v12" }], ["path", { d: "m14 9 4-4 4 4" }], ["path", { d: "m2 15 4 4 4-4" }]]);
export const PanelBottomIcon = createIcon("底部面板", [["rect", { x: 3, y: 3, width: 18, height: 18, rx: 2 }], ["path", { d: "M3 15h18" }]]);
export const SendIcon = createIcon("发送", [["path", { d: "m22 2-7 20-4-9-9-4z" }], ["path", { d: "M22 2 11 13" }]]);
export const RefreshIcon = createIcon("重连", [["path", { d: "M20 7h-5V2" }], ["path", { d: "M20 7a8 8 0 1 0 1 8" }]]);
export const EyeIcon = createIcon("显示", [["path", { d: "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12" }], ["circle", { cx: 12, cy: 12, r: 3 }]]);
export const EyeOffIcon = createIcon("隐藏", [["path", { d: "m3 3 18 18" }], ["path", { d: "M10.6 5.2A10.7 10.7 0 0 1 12 5c6.5 0 10 7 10 7a17.7 17.7 0 0 1-2.1 3.2" }], ["path", { d: "M6.6 6.6C3.7 8.4 2 12 2 12s3.5 7 10 7a9.8 9.8 0 0 0 5.4-1.6" }]]);
