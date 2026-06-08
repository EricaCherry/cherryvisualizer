// Butterchurn ships without TypeScript types; declare the small surface we use.
declare module 'butterchurn' {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const butterchurn: any;
  export default butterchurn;
}
declare module 'butterchurn-presets' {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const presets: any;
  export default presets;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  export function getPresets(): any;
}
