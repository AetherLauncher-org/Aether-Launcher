export const defaultPort: number;
export function canBind(port: number, host: string): Promise<boolean>;
export function isFree(port: number): Promise<boolean>;
export function findPort(
  start?: number,
  tries?: number,
  free?: (port: number) => Promise<boolean>,
): Promise<number>;
