export type VirtualWindow = {
  start: number
  end: number
  offsetTop: number
  totalHeight: number
}

export const calculateVirtualWindow = (
  itemCount: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  overscan: number,
): VirtualWindow => {
  if (itemCount <= 0 || rowHeight <= 0) {
    return { start: 0, end: 0, offsetTop: 0, totalHeight: 0 }
  }
  const safeViewport = Math.max(0, viewportHeight)
  const firstVisible = Math.max(0, Math.floor(Math.max(0, scrollTop) / rowHeight))
  const visibleCount = Math.max(1, Math.ceil(safeViewport / rowHeight))
  const start = Math.max(0, firstVisible - Math.max(0, overscan))
  const end = Math.min(itemCount, firstVisible + visibleCount + Math.max(0, overscan))
  return {
    start,
    end,
    offsetTop: start * rowHeight,
    totalHeight: itemCount * rowHeight,
  }
}
