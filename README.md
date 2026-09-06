# Mouse Insight

Windows 鼠标按键映射工具。先看见系统刚收到的是哪颗键，再把它绑成键盘快捷键。没列出的键一律放行。

适合游戏鼠标侧键、狼途这类「系统能收到、软件听不到」的键。常见用法：把侧键按住映射成 Typeless 听写用的 `Left Ctrl + Left Alt`。

![Mouse Insight](docs/screenshot.png)

<p>
  <img src="https://img.shields.io/badge/Windows-10%20%2F%2011-0A84FF" alt="Windows 10 / 11" />
  <img src="https://img.shields.io/github/v/release/lingcang728/MouseInsight?include_prereleases&label=release" alt="Release" />
  <img src="https://img.shields.io/github/license/lingcang728/MouseInsight" alt="MIT" />
</p>

## 能做什么

- **先听再绑**：点「开始听」，按鼠标上任意一颗键，界面会标出刚按下的是哪颗。
- **映射成快捷键**：把侧键、中键、滚轮绑成任意键盘组合。
- **三种触发方式**：按住、点按、开关。
- **左键右键永远放行**：不会被程序吞掉，避免锁死其它窗口。
- **紧急解除**：按 `Pause` 或 `Scroll Lock`，立刻停止拦截。
- **冲突提示**：检测到 [X-Mouse Button Control](https://www.highrez.co.uk/downloads/xmousebuttoncontrol.htm) 还在跑时会提醒先退出。两套鼠标钩子不能一起开。
- **托盘常驻**：关窗口会藏到托盘；开机自启可选；配置和程序放在一起，不联网。

## 安装

到 [Releases](https://github.com/lingcang728/MouseInsight/releases) 下载最新安装包，双击安装。装到当前用户目录，不需要管理员权限。

安装包暂未代码签名。Windows SmartScreen 可能提示「未知发布者」，点「更多信息」→「仍要运行」即可。

## 用法

1. 若正在使用 X-Mouse Button Control，先完全退出它。
2. 打开 Mouse Insight，点 **开始听**，按下要绑定的那颗鼠标键（侧键、中键、滚轮都可以）。
3. 在弹出的录制层同时按住要发出的键盘组合，点 **就这个**。录制时不会触发 Typeless 等全局热键。
4. 之后按这颗鼠标键，就会发出对应快捷键。

映射行可以随时改键、改模式、重录快捷键或删除。没列出的键 Windows 原样处理。

关闭窗口会藏到托盘，单击托盘图标再打开。要真正退出，用窗口里的「退出」或托盘菜单。

## 映射模式

| 模式 | 行为 |
|------|------|
| **按住** | 按着鼠标键就按着快捷键，松开就松开。适合修饰键，例如 `Ctrl + Alt`。 |
| **点按** | 按下鼠标键时点一下快捷键。适合 `Enter` 这类一次性按键。 |
| **开关** | 按一下按下并保持，再按一下松开。适合需要保持的状态。 |

可拦截并改写的键：侧键 · 后、侧键 · 前、中键、滚轮上、滚轮下。左键和右键可以听，但不会被拦截。

## 配置与隐私

配置文件是 `config.json`，和 `Mouse Insight.exe` 在同一目录。换电脑时把这个文件一起拷过去即可。

程序只在本地运行，不联网，不上传按键。

## 从源码构建

需要 [Node.js](https://nodejs.org/) 与 [Rust](https://rustup.rs/)。

```powershell
npm install
npm run tauri:dev      # 开发
npm run package:release  # 产出安装包到 release\
```

## 许可证

[MIT](LICENSE)
