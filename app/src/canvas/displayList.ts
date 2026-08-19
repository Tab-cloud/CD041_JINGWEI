import type { DisplayPrimitive, Point, Viewport } from "../clients/contracts";

export interface PrimitiveBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export function primitiveBounds(primitive: DisplayPrimitive): PrimitiveBounds {
  if (primitive.points && primitive.points.length > 0) {
    const xs = primitive.points.map((point) => point.x);
    const ys = primitive.points.map((point) => point.y);
    const padding = Math.max(primitive.width ?? 2, primitive.radius ?? 0, 4);
    return {
      left: Math.min(...xs) - padding,
      top: Math.min(...ys) - padding,
      right: Math.max(...xs) + padding,
      bottom: Math.max(...ys) + padding,
    };
  }
  const position = primitive.position ?? { x: 0, y: 0 };
  const halfWidth = (primitive.width ?? (primitive.radius ?? 5) * 2) / 2;
  const halfHeight = (primitive.height ?? (primitive.radius ?? 5) * 2) / 2;
  return {
    left: position.x - halfWidth,
    top: position.y - halfHeight,
    right: position.x + halfWidth,
    bottom: position.y + halfHeight,
  };
}

export function cullDisplayList(primitives: DisplayPrimitive[], viewport: Viewport): DisplayPrimitive[] {
  const visible: PrimitiveBounds = {
    left: viewport.x,
    top: viewport.y,
    right: viewport.x + viewport.width / viewport.zoom,
    bottom: viewport.y + viewport.height / viewport.zoom,
  };
  return primitives
    .filter((primitive) => intersects(primitiveBounds(primitive), visible))
    .sort((left, right) => left.zIndex - right.zIndex);
}

export function hitDistance(primitive: DisplayPrimitive, point: Point): number {
  if (primitive.points && primitive.points.length > 1) {
    let distance = Number.POSITIVE_INFINITY;
    for (let index = 1; index < primitive.points.length; index += 1) {
      const start = primitive.points[index - 1];
      const end = primitive.points[index];
      if (start && end) distance = Math.min(distance, distanceToSegment(point, start, end));
    }
    return distance;
  }
  const bounds = primitiveBounds(primitive);
  const dx = Math.max(bounds.left - point.x, 0, point.x - bounds.right);
  const dy = Math.max(bounds.top - point.y, 0, point.y - bounds.bottom);
  return Math.hypot(dx, dy);
}

export function canvasPointToWorld(
  clientPoint: Point,
  canvasRect: Pick<DOMRect, "left" | "top">,
  viewport: Pick<Viewport, "x" | "y" | "zoom">,
): Point {
  return {
    x: viewport.x + (clientPoint.x - canvasRect.left) / viewport.zoom,
    y: viewport.y + (clientPoint.y - canvasRect.top) / viewport.zoom,
  };
}

export function buildSyntheticDisplayList(count: number): DisplayPrimitive[] {
  const columns = Math.ceil(Math.sqrt(count));
  return Array.from({ length: count }, (_, index) => ({
    id: `synthetic-${index}`,
    entityId: `synthetic-${index}`,
    kind: index % 4 === 0 ? "via" : "pad",
    layerId: index % 2 === 0 ? "F.Cu" : "B.Cu",
    position: { x: (index % columns) * 14, y: Math.floor(index / columns) * 14 },
    width: 8,
    height: 8,
    radius: 4,
    zIndex: 10,
  } satisfies DisplayPrimitive));
}

function intersects(left: PrimitiveBounds, right: PrimitiveBounds): boolean {
  return left.left <= right.right && left.right >= right.left && left.top <= right.bottom && left.bottom >= right.top;
}

function distanceToSegment(point: Point, start: Point, end: Point): number {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  if (dx === 0 && dy === 0) return Math.hypot(point.x - start.x, point.y - start.y);
  const t = Math.max(0, Math.min(1, ((point.x - start.x) * dx + (point.y - start.y) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(point.x - (start.x + t * dx), point.y - (start.y + t * dy));
}
