import type { DisplayPrimitive, Point, Viewport } from "../clients/contracts";

export interface RenderOptions {
  viewport: Viewport;
  selectedIds: Set<string>;
  relatedIds: Set<string>;
  ghostPrimitives: DisplayPrimitive[];
  devicePixelRatio: number;
}

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
  drawGrid(context, rect.width, rect.height, options.viewport);
  context.save();
  context.scale(options.viewport.zoom, options.viewport.zoom);
  context.translate(-options.viewport.x, -options.viewport.y);
  for (const primitive of primitives) drawPrimitive(context, primitive, options);
  for (const ghost of options.ghostPrimitives) drawPrimitive(context, ghost, options, true);
  context.restore();
}

function drawGrid(context: CanvasRenderingContext2D, width: number, height: number, viewport: Viewport): void {
  const minor = Math.max(12, 20 * viewport.zoom);
  const offsetX = -((viewport.x * viewport.zoom) % minor);
  const offsetY = -((viewport.y * viewport.zoom) % minor);
  context.strokeStyle = "rgba(132, 151, 177, 0.07)";
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
  const selected = options.selectedIds.has(primitive.entityId);
  const related = options.relatedIds.has(primitive.entityId) || Boolean(primitive.netId && options.relatedIds.has(primitive.netId));
  const color = selected ? "#f5c66d" : related ? "#6cd1c0" : (primitive.color ?? "#8ca3c7");
  context.strokeStyle = color;
  context.fillStyle = color;
  context.lineWidth = selected ? 3 / options.viewport.zoom : 1.6 / options.viewport.zoom;

  switch (primitive.kind) {
    case "symbol":
      drawRectEntity(context, primitive, color, "rgba(37, 51, 71, 0.92)");
      break;
    case "footprint":
      drawRectEntity(context, primitive, color, "rgba(82, 35, 49, 0.28)");
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
      drawCircle(context, primitive);
      break;
    case "finding":
      drawFinding(context, primitive);
      break;
    case "label":
    case "text":
      drawText(context, primitive, color);
      break;
  }
  if ((selected || related) && !["wire", "track", "ratsnest", "board-outline"].includes(primitive.kind)) {
    drawSelectionHalo(context, primitive, selected ? "#f5c66d" : "#6cd1c0");
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

function drawCircle(context: CanvasRenderingContext2D, primitive: DisplayPrimitive): void {
  const position = primitive.position ?? { x: 0, y: 0 };
  context.beginPath();
  context.arc(position.x, position.y, primitive.radius ?? 4, 0, Math.PI * 2);
  context.fill();
  if (primitive.kind === "via") {
    context.fillStyle = "#17202d";
    context.beginPath();
    context.arc(position.x, position.y, (primitive.radius ?? 8) * 0.42, 0, Math.PI * 2);
    context.fill();
  }
}

function drawFinding(context: CanvasRenderingContext2D, primitive: DisplayPrimitive): void {
  const position = primitive.position ?? { x: 0, y: 0 };
  const radius = primitive.radius ?? 16;
  context.strokeStyle = primitive.severity === "error" ? "#ef6d67" : "#efb366";
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
  context.font = `${primitive.kind === "text" ? 12 : 11}px Inter, "Segoe UI", sans-serif`;
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
