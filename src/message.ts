export function isErrorMessage(text: string): boolean {
  return /失败|错误|无法|拒绝|异常|已拒绝|超时|fail|error|denied|refused|timeout|invalid/i.test(text)
}
