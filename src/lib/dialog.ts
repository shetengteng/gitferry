/** 打开系统目录选择器，返回所选目录绝对路径；用户取消时返回 null */
export async function pickDirectory(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}
