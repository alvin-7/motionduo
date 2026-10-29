package locale

import (
	"fmt"
	"strings"
)

// Language is the active UI language, "en" or "zh-Hans".
var Language = "en"

// Tr returns english translated into Language, or english itself.
func Tr(english string) string {
	if Language == "zh-Hans" {
		if translated, ok := SimplifiedChinese[english]; ok {
			return translated
		}
	}
	return english
}

// Trf formats the translation of english.
func Trf(english string, args ...any) string { return fmt.Sprintf(Tr(english), args...) }

func LanguageForLocale(locale string) string {
	locale = strings.ToLower(strings.ReplaceAll(strings.TrimSpace(locale), "_", "-"))
	locale = strings.Split(strings.Split(locale, ".")[0], "@")[0]
	if locale == "zh" || locale == "zh-cn" || locale == "zh-sg" || strings.HasPrefix(locale, "zh-hans") {
		return "zh-Hans"
	}
	return "en"
}

func FromEnvironment(getenv func(string) string) string {
	for _, key := range []string{"LC_ALL", "LC_MESSAGES", "LANG"} {
		if value := strings.TrimSpace(getenv(key)); value != "" {
			return value
		}
	}
	return ""
}

var SimplifiedChinese = map[string]string{
	"Disable %s first":                       "请先关闭 %s",
	"Cancel":                                 "取消",
	"This computer":                          "本机",
	"Could not save device alias; try again": "无法保存设备别名，请重试",
	"Edit alias":                             "编辑别名", "Local device alias": "本机设备别名", "Alias (empty to clear)": "别名（留空清除）", "Save alias": "保存别名", "Cancel alias": "取消编辑别名", "Saving alias...": "正在保存别名…",
	"Alias was not confirmed; try again": "别名未获确认，请重试", "Alias changed elsewhere; try again": "别名已在其他位置更改，请重试",
	"Enter an alias of at most 128 characters without control characters": "请输入不含控制字符的别名（最多 128 个字符）",
	"About": "关于", "Version %s": "版本 %s", "Maintainer": "维护者",
	"Based on Mousehop and lan-mouse":                      "基于 Mousehop 与 lan-mouse 开发",
	"GPL-3.0-or-later; upstream copyrights are preserved.": "采用 GPL-3.0-or-later，保留上游版权声明。",
	"Devices": "设备", "Screen layout": "屏幕布局", "Settings": "设置", "Pages": "页面",
	"Reconnecting": "正在重新连接", "Ready": "就绪", "Input access needed": "需要输入权限", "Restart needed": "需要重启", "Dismiss": "关闭提示",
	"Discovery email": "发现邮箱", "Local discovery": "局域网发现", "Email address": "邮箱地址", "Save email": "保存邮箱", "Saving...": "正在保存…", "Saved": "已保存",
	"Use the same email on your computers. No account needed.": "在各台电脑上使用相同邮箱，无需注册账户。",
	"%s requests control of this computer":                     "%s 请求控制这台电脑", "Decline": "拒绝", "Allow": "允许", "Computers you control": "你控制的电脑", "Configure %s": "配置 %s", "Share with %s": "与 %s 共享", "Nearby": "附近设备", "Connect": "连接", "Connecting...": "正在连接…", "Set discovery email": "设置发现邮箱", "No other computers yet": "暂无其他电脑", "Open MotionDuo on another computer using the same email.": "在另一台电脑上打开 MotionDuo，并使用相同邮箱。", "Enable local discovery in Settings.": "请在设置中启用局域网发现。", "Computer": "电脑",
	"Input access": "输入权限", "Enable input capture": "启用输入捕获", "Enable input emulation": "启用输入模拟", "Advanced": "高级", "Network": "网络", "Listen port": "监听端口", "Apply port": "应用端口", "Connect by address": "通过地址连接", "Computer address": "电脑地址", "Hostname or IP address": "主机名或 IP 地址", "Remote port": "远程端口", "Add computer": "添加电脑", "Clipboard privacy": "剪贴板隐私", "Remove": "移除", "Private application": "隐私应用", "Application identifier": "应用标识符", "Exclude app": "排除应用", "Running apps": "正在运行的应用",
	"Clipboard sync": "剪贴板同步", "Configure": "配置", "Reliable input (KCP)": "可靠输入（KCP）", "Trackpad momentum": "触控板惯性滚动", "Command as Control": "将 Command 用作 Control", "Hold modifier to cross": "按住修饰键跨屏", "Crossing modifier": "跨屏修饰键", "Remove computer": "移除电脑", "Computers allowed to control you": "获准控制你的电脑", "Locked - unlock locally": "已锁定，请在本机解锁", "Configure incoming %s": "配置传入设备 %s", "Natural scrolling": "自然滚动", "Pointer sensitivity": "指针灵敏度", "Remove access": "撤销访问权限",
	"Connection details": "连接详情", "%s connection details": "%s 连接详情", "Not connected": "未连接", "Connected to %s": "已连接至 %s", "Resolving hostname...": "正在解析主机名…", "Pinned on this network; no automatic failover.": "已固定此网络，不会自动切换。", "Saving connection...": "正在保存连接…", "Edit connection": "编辑连接", "Refresh addresses": "刷新地址", "Remote hostname": "远程主机名", "Hostname (optional with fixed IPs)": "主机名（设置固定 IP 后可留空）", "Connection port": "连接端口", "Fixed IP addresses": "固定 IP 地址", "Fixed IPv4 / IPv6 addresses, comma-separated": "固定 IPv4 / IPv6 地址，以逗号分隔", "Enter command": "进入时执行命令", "Optional local command (sh -c)": "可选本机命令（sh -c）", "Save connection": "保存连接", "Cancel connection": "取消编辑连接", "Prefer lowest latency": "优先最低延迟", "Network path": "网络路径", "Automatic": "自动", " / Saving...": " / 正在保存…", " / Wired": " / 有线", " / Wi-Fi": " / Wi-Fi", " / Other": " / 其他", "Not measured": "未测量", "Unreachable": "无法访问",
	"Return control": "交还控制", "Waiting for daemon": "等待后台服务", "Edit shortcut": "编辑快捷键", "Wall-press fallback (px)": "顶边回退阈值（像素）", "Wall-press fallback pixels": "顶边回退像素数", "Apply threshold": "应用阈值", "0 disables. Uses estimated motion; increase if control returns too early.": "设为 0 可关闭。基于估算移动量；若控制过早返回，请增大阈值。", "Trigger key": "触发键", "%s modifier": "%s 修饰键", "Apply shortcut": "应用快捷键", "Cancel shortcut": "取消编辑快捷键", "Use default": "使用默认值",
	"Connect a computer first": "请先连接一台电脑", "Find computers": "查找电脑", "Screen arrangement": "屏幕排列", "Local screen": "本机屏幕", "Remote screen": "远程屏幕", "Top left": "左上角", "Top right": "右上角", "Bottom left": "左下角", "Bottom right": "右下角", "Drag the blue screen to arrange": "拖动蓝色屏幕调整布局", "Saving arrangement…": "正在保存布局…", "Reconnect to arrange screens": "重新连接后可调整屏幕",
	"Allow MotionDuo in macOS Accessibility.": "请在 macOS 辅助功能中允许 MotionDuo。", "Allow MotionDuo in macOS Input Monitoring.": "请在 macOS 输入监控中允许 MotionDuo。", "Re-enable MotionDuo in macOS Accessibility.": "请在 macOS 辅助功能中重新启用 MotionDuo。", "Access updated. Restart MotionDuo to apply.": "权限已更新，请重启 MotionDuo 以应用。", "Cannot check input access: %s": "无法检查输入权限：%s", "Restart MotionDuo": "重启 MotionDuo", "Full Disk Access": "完全磁盘访问", "Needed to sync copied image files.": "同步复制的图片文件需要此权限。", "Open settings": "打开系统设置", "Check again": "重新检查", "Open MotionDuo": "打开 MotionDuo", "Open at login": "登录时启动", "Quit": "退出",
	"Request again": "重新请求", "Approval required": "需要重新授权",
	"Connected": "已连接", "Device identity needs confirmation": "需要确认设备身份", "Input release needs attention": "请检查输入释放状态", "Connection timed out": "连接超时", "Connection failed": "连接失败", "Waiting for connection": "等待连接",
	"Connection settings were not confirmed; try again": "连接设置未获确认，请重试", "Network choice was not confirmed; try again": "网络选择未获确认，请重试", "Shortcut was not confirmed; try again": "快捷键未获确认，请重试", "Release threshold was not confirmed; try again": "回退阈值未获确认，请重试", "Screen layout was not confirmed; try again": "屏幕布局未获确认，请重试", "Email was not confirmed; try again": "邮箱未获确认，请重试", "Connection request was not confirmed; try again": "连接请求未获确认，请重试", "Waiting for a compatible daemon": "等待兼容的后台服务",
	"Enter a hostname without spaces (253 bytes maximum)": "请输入不含空格的主机名（最多 253 字节）", "Enter a port from 1 to 65535": "请输入 1 至 65535 之间的端口", "Invalid fixed IP address: %s": "无效的固定 IP 地址：%s", "Enter a hostname or at least one fixed IP address": "请输入主机名或至少一个固定 IP 地址", "The enter command cannot contain a NUL character": "进入命令不能包含 NUL 字符",
	"Input service stopped. Restart MotionDuo from the tray or menu bar.": "输入服务已停止，请从托盘或菜单栏重启 MotionDuo。", "Incompatible daemon. Quit the other MotionDuo instance, then reopen this build.": "后台服务不兼容。请退出其他 MotionDuo 实例后重新打开此版本。",
}
