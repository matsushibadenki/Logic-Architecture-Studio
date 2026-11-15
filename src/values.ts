/** Values cross IPC as strings to preserve all 64 bits. */
export function formatValue(
  value: string,
  radix: number,
  width = 64,
  signed = false,
) {
  const bits = BigInt.asUintN(width, BigInt(value));
  return radix === 16
    ? `0x${bits
        .toString(16)
        .toUpperCase()
        .padStart(Math.ceil(width / 4), "0")}`
    : (signed ? BigInt.asIntN(width, bits) : bits).toString();
}
