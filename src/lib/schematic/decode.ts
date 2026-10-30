/**
 * 后端发来的方块数据：游程编码的调色板序号，每一段是一对小端 `u32`（序号、连续个数），
 * 整体再套一层 base64。展开成每个方块一个 `u16`（调色板最多 65536 项）。
 */

export function decodeRuns(base64: string, runCount: number, volume: number): Uint16Array {
  const binary = atob(base64);
  if (binary.length !== runCount * 8) {
    throw new Error("方块数据的长度和段数对不上");
  }
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  const view = new DataView(bytes.buffer);
  const blocks = new Uint16Array(volume);
  let position = 0;
  for (let run = 0; run < runCount; run += 1) {
    const value = view.getUint32(run * 8, true);
    const count = view.getUint32(run * 8 + 4, true);
    if (value > 0xffff || count === 0 || position + count > volume) {
      throw new Error("方块数据超出了范围");
    }
    blocks.fill(value, position, position + count);
    position += count;
  }
  if (position !== volume) {
    throw new Error("方块数据不够");
  }
  return blocks;
}
