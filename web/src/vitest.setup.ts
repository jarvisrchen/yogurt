import "@testing-library/jest-dom/vitest";

// jsdom has no ResizeObserver, and nothing in jsdom resizes anyway.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
