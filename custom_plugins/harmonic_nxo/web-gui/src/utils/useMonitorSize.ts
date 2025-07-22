import { throttle } from "lodash";
import { type RefObject, useState, useEffect, useMemo } from "react";

export type MeasurableElement = HTMLElement & {
  getBoundingClientRect(): DOMRect;
  offsetTop: number;
  offsetLeft: number;
  offsetHeight: number;
  offsetWidth: number;
};

export type BBox = {
  top: number;
  left: number;
  width: number;
  height: number;
};

function getBBoxForElement<T extends MeasurableElement>(
  element: T,
  ignoreTransforms = false
): BBox {
  if (ignoreTransforms) {
    return {
      top: element.offsetTop,
      left: element.offsetLeft,
      width: element.offsetWidth,
      height: element.offsetHeight,
    };
  }
  const rect = element.getBoundingClientRect();
  return {
    top: rect.top,
    left: rect.left,
    width: rect.width,
    height: rect.height,
  };
}

/**
 * Hook to monitor the bounding box (top, left, width, height) of a DOM element.
 * Uses ResizeObserver if available; otherwise, falls back to a throttled window resize event.
 *
 * @param elementRef - Ref to the element to measure.
 * @param throttleMillis - Throttle interval in milliseconds (default: 100).
 * @param ignoreTransforms - Ignore the effects of CSS transforms on the element's position (default: false).
 * @returns The current bounding box of the element, or null if the element isn’t available.
 */
export default function useMonitorSize<T extends MeasurableElement>(
  elementRef: RefObject<T | null>,
  throttleMillis: number = 100,
  ignoreTransforms: boolean = false
): BBox | null {
  const [bbox, setBBox] = useState<BBox | null>(null);


  // Create a throttled measurement function that checks and updates the bounding box.
  const throttledMeasure = useMemo(
    () =>
      throttle(
        () => {

          const element = elementRef.current;
          if (element) {
            const newBBox = getBBoxForElement(element, ignoreTransforms);
            const prev = bbox;
            if (
              !prev ||
              newBBox.top !== prev.top ||
              newBBox.left !== prev.left ||
              newBBox.width !== prev.width ||
              newBBox.height !== prev.height
            ) {
              setBBox(newBBox);
            }
          }
        },
        throttleMillis,
        { leading: true, trailing: true }
      ),
    [throttleMillis, bbox]
  );

  useEffect(() => {
    const element = elementRef.current;
    if (!element) return;

    let observer: ResizeObserver | null = null;

    // If available, use ResizeObserver to monitor element size changes.
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(() => {
        throttledMeasure();
      });
      observer.observe(element);
    }

    // Always listen for window resize events as a fallback.
    window.addEventListener("resize", throttledMeasure);

    // Run an initial measurement.
    throttledMeasure();



    return () => {
      if (observer) {
        observer.disconnect();
      }
      window.removeEventListener("resize", throttledMeasure);
      throttledMeasure.cancel();
    };
  }, [throttledMeasure]);


  useEffect(() => {
    if(bbox === null) {
      throttledMeasure();
    }
  })

  return bbox;
}

export function combineBBoxes(...boxes: (BBox | null)[]): BBox | null {
  if (boxes.some((box) => box === null)) {
    return null;
  }
  const top = Math.min(...boxes.map((box) => box?.top ?? Infinity));
  const left = Math.min(...boxes.map((box) => box?.left ?? Infinity));
  const width = Math.max(...boxes.map((box) => box?.width ?? 0)) - left;
  const height = Math.max(...boxes.map((box) => box?.height ?? 0)) - top;
  return { top, left, width, height };
}
