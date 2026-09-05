/** 打开系统目录选择器，返回所选目录绝对路径；用户取消时返回 null */
export async function pickDirectory(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

/** 弹出系统确认对话框（warning 样式），用户点「确认开启」返回 true，取消返回 false */
export async function askConfirm(title: string, message: string): Promise<boolean> {
  const { ask } = await import("@tauri-apps/plugin-dialog");
  return ask(message, { title, kind: "warning", okLabel: "确认开启", cancelLabel: "取消" });
}

