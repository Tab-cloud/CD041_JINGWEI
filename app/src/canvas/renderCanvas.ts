import type { DisplayPrimitive, Point, Viewport } from "../clients/contracts";
import type { Theme } from "../theme/theme";

export interface RenderOptions {
  viewport: Viewport;
  selectedIds: Set<string>;
  relatedIds: Set<string>;
  ghostPrimitives: DisplayPrimitive[];
  devicePixelRatio: number;
  theme: Theme;
}

interface CanvasPalette {
  grid: string;
  selected: string;
  related: string;
  primitive: string;
  symbolFill: string;
  footprintFill: string;
  viaCore: string;
  error: string;
  warning: string;
}

const canvasPalettes: Record<Theme, CanvasPalette> = {
  dark: {
    grid: "rgba(174, 184, 199, 0.075)",
    selected: "#ff9f0a",
    related: "#64d2ff",
    primitive: "#8eacd3",
    symbolFill: "rgba(38, 45, 56, 0.94)",
    footprintFill: "rgba(255, 69, 58, 0.15)",
    viaCore: "#17191d",
    error: "#ff6961",
    warning: "#ffb340",
  },
  light: {
    grid: "rgba(60, 60, 67, 0.105)",
    selected: "#d96c00",
    related: "#007f8f",
    primitive: "#4f6f95",
    symbolFill: "rgba(255, 255, 255, 0.96)",
    footprintFill: "rgba(255, 59, 48, 0.09)",
    viaCore: "#f2f2f7",
    error: "#d70015",
    warning: "#b25000",
  },
};

export function renderCanvas(
  canvas: HTMLCanvasElement,
  primitives: DisplayPrimitive[],
  options: RenderOptions,
): void {
  const context = canvas.getContext("2d");
  if (!context) return;
  const rect = canvas.getBoundingClientRect();
  const pixelRatio = Math.max(1, options.devicePixelRatio);
  const width = Math.max(1, Math.round(rect.width * pixelRatio));
  const height = Math.max(1, Math.round(rect.height * pixelRatio));
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }

  context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  context.clearRect(0, 0, rect.width, rect.height);
  const palette = canvasPalettes[options.theme];
  drawGrid(context, rect.width, rect.height, options.viewport, palette.grid);
  context.save();
  context.scale(options.viewport.zoom, options.viewport.zoom);
  context.translate(-options.viewport.x, -options.viewport.y);
  for (const primitive of primitives) drawPrimitive(context, primitive, options);
  for (const ghost of options.ghostPrimitives) drawPrimitive(context, ghost, options, true);
  context.restore();
}

function drawGrid(context: CanvasRenderingContext2D, width: number, height: number, viewport: Viewport, color: string): void {
  const minor = Math.max(12, 20 * viewport.zoom);
  const offsetX = -((viewport.x * viewport.zoom) % minor);
  const offsetY = -((viewport.y * viewport.zoom) % minor);
  context.strokeStyle = color;
  context.lineWidth = 1;
  context.beginPath();
  for (let x = offsetX; x < width; x += minor) {
    context.moveTo(x, 0);
    context.lineTo(x, height);
  }
  for (let y = offsetY; y < height; y += minor) {
    context.moveTo(0, y);
    context.lineTo(width, y);
  }
  context.stroke();
}

function drawPrimitive(
  context: CanvasRenderingContext2D,
  primitive: DisplayPrimitive,
  options: RenderOptions,
  ghost = false,
): void {
  context.save();
  context.globalAlpha = ghost ? 0.72 : 1;
  const palette = canvasPalettes[options.theme];
  const selected = options.selectedIds.has(primitive.entityId);
  const related = options.relatedIds.has(primitive.entityId) || Boolean(primitive.netId && options.relatedIds.has(primitive.netId));
  const color = selected ? palette.selected : related ? palette.related : (primitive.color ?? palette.primitive);
  context.strokeStyle = color;
  context.fillStyle = color;
  context.lineWidth = selected ? 3 / options.viewport.zoom : 1.6 / options.viewport.zoom;

  switch (primitive.kind) {
    case "symbol":
      drawRectEntity(context, primitive, color, palette.symbolFill);
      break;
    case "footprint":
      drawRectEntity(context, primitive, color, palette.footprintFill);
      drawFootprintOrigin(context, primitive);
      break;
    case "pad":
      drawFilledRect(context, primitive);
      break;
    case "wire":
    case "track":
    case "ratsnest":
    case "board-outline":
      drawPolyline(context, primitive, primitive.kind === "ratsnest");
      break;
    case "zone":
      drawZone(context, primitive);
      break;
    case "pin":
    case "junction":
    case "via":
      drawCircle(context, primitive, palette);
      break;
    case "finding":
      drawFinding(context, primitive, palette);
      break;
    case "label":
    case "text":
      drawText(context, primitive, color);
      break;
  }
  if ((selected || related) && !["wire", "track", "ratsnest", "board-outline"].includes(primitive.kind)) {
    drawSelectionHalo(context, primitive, selected ? palette.selected : palette.related);
  }
  context.restore();
}

function drawRectEntity(context: CanvasRenderingContext2D, primitive: DisplayPrimitive, stroke: string, fill: string): void {
  const position = primitive.position ?? { x: 0, y: 0 };
  const width = primitive.width ?? 80;
  const height = primitive.height ?? 50;
  context.fillStyle = fill;
  context.strokeStyle = stroke;
  context.beginPath();
  context.roundRect(position.x - width / 2, position.y - height / 2, width, height, 5);
  context.fill();
  context.stroke();
  drawText(context, primitive, stroke);
  if (primitive.locked) {
    context.beginPath();
    context.moveTo(position.x + width / 2 - 18, position.y - height / 2 + 15);
    context.lineTo(position.x + width / 2 - 6, position.y - height / 2 + 15);
    context.stroke();
  }
}

function drawFilledRect(context: CanvasRenderingContext2D, primitive: DisplayPrimitive): void {
  const position = primitive.position ?? { x: 0, y: 0 };
  const width = primitive.width ?? 10;
  const height = primitive.height ?? 10;
  context.fillRect(position.x - width / 2, position.y - height / 2, width, height);
}

function drawFootprintOrigin(context: CanvasRenderingContext2D, primitive: DisplayPrimitive): void {
  const position = primitive.position;
  if (!position) return;
  context.beginPath();
  context.moveTo(position.x - 6, position.y);
  context.lineTo(position.x + 6, position.y);
  context.moveTo(position.x, position.y - 6);
  context.lineTo(position.x, position.y + 6);
  context.stroke();
}

function drawPolyline(context: CanvasRenderingContext2D, primitive: DisplayPrimitive, dashed: boolean): void {
  const points = primitive.points ?? [];
  if (points.length < 2) return;
  context.lineWidth = Math.max(context.lineWidth, primitive.width ?? (dashed ? 1 : 2));
  if (dashed) context.setLineDash([6, 5]);
  context.beginPath();
  points.forEach((point, index) => index === 0 ? context.moveTo(point.x, point.y) : context.lineTo(point.x, point.y));
  context.stroke();
}

function drawZone(context: CanvasRenderingContext2D, primitive: DisplayPrimitive): void {
  const points = primitive.points ?? [];
  if (points.length < 3) return;
  context.globalAlpha *= 0.12;
  context.beginPath();
  points.forEach((point, index) => index === 0 ? context.moveTo(point.x, point.y) : context.lineTo(point.x, point.y));
  context.closePath();
  context.fill();
  context.stroke();
}

function drawCircle(context: CanvasRenderingContext2D, primitive: DisplayPrimitive, palette: CanvasPalette): void {
  const position = primitive.position ?? { x: 0, y: 0 };
  context.beginPath();
  context.arc(position.x, position.y, primitive.radius ?? 4, 0, Math.PI * 2);
  context.fill();
  if (primitive.kind === "via") {
    context.fillStyle = palette.viaCore;
    context.beginPath();
    context.arc(position.x, position.y, (primitive.radius ?? 8) * 0.42, 0, Math.PI * 2);
    context.fill();
  }
}

function drawFinding(context: CanvasRenderingContext2D, primitive: DisplayPrimitive, palette: CanvasPalette): void {
  const position = primitive.position ?? { x: 0, y: 0 };
  const radius = primitive.radius ?? 16;
  context.strokeStyle = primitive.severity === "error" ? palette.error : palette.warning;
  context.lineWidth = 3;
  context.beginPath();
  context.arc(position.x, position.y, radius, 0, Math.PI * 2);
  context.stroke();
  context.beginPath();
  context.moveTo(position.x, position.y - radius * 0.5);
  context.lineTo(position.x, position.y + radius * 0.2);
  context.stroke();
  context.beginPath();
  context.arc(position.x, position.y + radius * 0.55, 1.5, 0, Math.PI * 2);
  context.fill();
}

function drawText(context: CanvasRenderingContext2D, primitive: DisplayPrimitive, color: string): void {
  if (!primitive.label || !primitive.position) return;
  context.fillStyle = color;
  context.font = `${primitive.kind === "text" ? 12 : 11}px -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif`;
  context.textAlign = "center";
  context.textBaseline = "middle";
  primitive.label.split("\n").forEach((line, index, lines) => {
    context.fillText(line, primitive.position!.x, primitive.position!.y + (index - (lines.length - 1) / 2) * 15);
  });
}

function drawSelectionHalo(context: CanvasRenderingContext2D, primitive: DisplayPrimitive, color: string): void {
  const position = primitive.position ?? primitive.points?.[0];
  if (!position) return;
  context.strokeStyle = color;
  context.setLineDash([4, 3]);
  context.beginPath();
  context.arc(position.x, position.y, Math.max(primitive.width ?? 0, primitive.height ?? 0, (primitive.radius ?? 6) * 2) / 2 + 7, 0, Math.PI * 2);
  context.stroke();
}

export function pointDelta(start: Point, end: Point): Point {
  return { x: end.x - start.x, y: end.y - start.y };
}
