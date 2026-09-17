import { describe, expect, it } from 'vitest'
import { calculateVirtualWindow } from './virtualization'

describe('calculateVirtualWindow', () => {
  it('returns an empty window for an empty list', () => {
    expect(calculateVirtualWindow(0, 100, 200, 34, 10)).toEqual({
      start: 0,
      end: 0,
      offsetTop: 0,
      totalHeight: 0,
    })
  })

  it('clamps negative scroll positions to the first row', () => {
    expect(calculateVirtualWindow(20, -100, 68, 34, 0)).toEqual({
      start: 0,
      end: 2,
      offsetTop: 0,
      totalHeight: 680,
    })
  })

  it('clamps a stale scroll position to the last full viewport after the list shrinks', () => {
    expect(calculateVirtualWindow(3, 10_000, 68, 34, 0)).toEqual({
      start: 1,
      end: 3,
      offsetTop: 34,
      totalHeight: 102,
    })
  })

  it('keeps overscan inside the available item range', () => {
    expect(calculateVirtualWindow(3, 10_000, 68, 34, 10)).toEqual({
      start: 0,
      end: 3,
      offsetTop: 0,
      totalHeight: 102,
    })
  })
})
