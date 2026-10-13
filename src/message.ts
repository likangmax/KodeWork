/** Error styling is a display hint derived from the message text, so it has to
 *  recognize every failure the interface can actually show. The keyword list
 *  below is exercised by a catalog-driven test in message.test.ts: adding a
 *  failure message that no keyword matches makes that test fail. */
export function isErrorMessage(text: string): boolean {
  return /失败|错误|无法|拒绝|异常|已拒绝|超时|不支持|需要凭据|请输入密码|请填写|需为|缺失|未检测到|不可用|fail|error|denied|refused|timeout|invalid|unsupported|not supported|unavailable|required|needs credential|connection lost|missing|not detected|not found|must be|please fill/i.test(text) // i18n-exempt: bilingual classification pattern, never displayed
}
